//! The tokio runtime: an `arc-swap` store with a broadcast delta stream, a single-writer
//! reconciler pump (OPEN sweep + drop→terminal fetch, adaptive cadence, demand-gated prices),
//! optimistic submit, on-demand refresh, and clean shutdown.
//!
//! The pump never awaits network I/O directly: it spawns detached tasks that feed results
//! back through the command channel, so the store has exactly one writer.

use crate::core::{Delta, OpKind, OptimisticOrder, OrderKey, ProjectedState};
use arc_swap::ArcSwap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tossinvest::{AccountClient, OrderCreateRequest, OrderId, OrderListFilter};
use tossinvest_model::{Order, PriceResponse};

/// Adaptive polling cadence (per lane tier).
#[derive(Clone, Debug)]
pub struct SchedulerConfig {
    /// Interval while an operation is in flight or an order is partially filled.
    pub hot: Duration,
    /// Interval while there are open orders.
    pub working: Duration,
    /// Interval when nothing is being tracked (still catches externally-created orders).
    pub idle: Duration,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            hot: Duration::from_millis(400),
            working: Duration::from_secs(1),
            idle: Duration::from_secs(5),
        }
    }
}

/// What to refresh on demand.
#[derive(Clone, Debug)]
pub enum RefreshTarget {
    /// Orders (an OPEN sweep) and holdings.
    All,
    /// Orders only (an OPEN sweep).
    Orders,
    /// Holdings only.
    Holdings,
    /// A single order by id.
    Order(OrderId),
    /// Prices for the given symbols.
    Prices(Vec<String>),
}

/// The result of an optimistic submission.
#[derive(Clone, Debug)]
pub struct SubmitOutcome {
    /// The local key the provisional row was inserted under.
    pub key: OrderKey,
}

// ── the store ────────────────────────────────────────────────────────────────────────

struct Store {
    state: ArcSwap<ProjectedState>,
    deltas: broadcast::Sender<Delta>,
    write: Mutex<ProjectedState>,
}

impl Store {
    fn new() -> Self {
        let (deltas, _) = broadcast::channel(256);
        Self {
            state: ArcSwap::from_pointee(ProjectedState::new()),
            deltas,
            write: Mutex::new(ProjectedState::new()),
        }
    }

    /// Apply a mutation: fold on the canonical copy, publish the snapshot, then notify.
    fn apply<F>(&self, f: F)
    where
        F: FnOnce(&mut ProjectedState) -> Vec<Delta>,
    {
        let (snapshot, deltas) = {
            let mut w = self.write.lock().unwrap();
            let deltas = f(&mut w);
            if deltas.is_empty() {
                return;
            }
            (Arc::new(w.clone()), deltas)
        };
        self.state.store(snapshot); // publish snapshot BEFORE notifying
        for d in deltas {
            let _ = self.deltas.send(d);
        }
    }

    fn snapshot(&self) -> Arc<ProjectedState> {
        self.state.load_full()
    }
}

// ── commands (all network results funnel back to the single-writer pump) ───────────────

enum Command {
    StartSweep,
    SweepDone {
        orders: Vec<Order>,
        dropped: Vec<Order>,
    },
    Observed(Box<Order>),
    Submit {
        local_id: u64,
        intent: Box<OptimisticOrder>,
        req: Box<OrderCreateRequest>,
    },
    SubmitResult {
        local_id: u64,
        result: Result<OrderId, String>,
    },
    Cancel {
        server_id: OrderId,
    },
    Prices(Vec<PriceResponse>),
    Refresh {
        target: RefreshTarget,
        reply: oneshot::Sender<u64>,
    },
    DemandChanged,
    /// Recompute cadence (sent after an off-command-path apply, e.g. a refresh).
    Wake,
}

// ── the handle ─────────────────────────────────────────────────────────────────────────

/// A cloneable handle to the stateful layer. The reconciler runs until the last clone drops.
#[derive(Clone)]
pub struct StateHandle {
    shared: Arc<Shared>,
}

struct Shared {
    store: Arc<Store>,
    commands: flume::Sender<Command>,
    cancel: CancellationToken,
    local_seq: AtomicU64,
    price_demand: Mutex<HashMap<String, usize>>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for StateHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateHandle").finish_non_exhaustive()
    }
}

impl StateHandle {
    /// Spawn the reconciler on top of an account-scoped client. Exactly one background task.
    pub fn spawn(account: AccountClient, config: SchedulerConfig) -> Self {
        let (tx, rx) = flume::unbounded();
        let store = Arc::new(Store::new());
        let cancel = CancellationToken::new();
        let shared = Arc::new(Shared {
            store: store.clone(),
            commands: tx.clone(),
            cancel: cancel.clone(),
            local_seq: AtomicU64::new(1),
            price_demand: Mutex::new(HashMap::new()),
            task: Mutex::new(None),
        });
        let task = tokio::spawn(pump(PumpCtx {
            store,
            account,
            commands: tx,
            rx,
            cancel,
            config,
            demand: shared.clone(),
        }));
        *shared.task.lock().unwrap() = Some(task);
        Self { shared }
    }

    /// A wait-free snapshot of the current state.
    pub fn snapshot(&self) -> Arc<ProjectedState> {
        self.shared.store.snapshot()
    }

    /// Subscribe to the delta stream. On `Lagged`, re-read [`StateHandle::snapshot`].
    pub fn subscribe(&self) -> broadcast::Receiver<Delta> {
        self.shared.store.deltas.subscribe()
    }

    /// Submit a create optimistically: a provisional row appears immediately; the real
    /// request runs in the background and confirms/fails the row.
    pub fn create_order(&self, request: OrderCreateRequest) -> SubmitOutcome {
        let local_id = self.shared.local_seq.fetch_add(1, Ordering::Relaxed);
        let intent = intent_of(&request);
        let _ = self.shared.commands.send(Command::Submit {
            local_id,
            intent: Box::new(intent),
            req: Box::new(request),
        });
        SubmitOutcome {
            key: OrderKey::Local(local_id),
        }
    }

    /// Submit a cancel for a server order id (optimistic `Closing` until observed).
    pub fn cancel_order(&self, server_id: OrderId) {
        let _ = self.shared.commands.send(Command::Cancel { server_id });
    }

    /// Force an immediate refresh; resolves with the snapshot generation once folded in.
    pub async fn refresh(&self, target: RefreshTarget) -> u64 {
        let (reply, rx) = oneshot::channel();
        if self
            .shared
            .commands
            .send(Command::Refresh { target, reply })
            .is_err()
        {
            return self.snapshot().generation;
        }
        rx.await.unwrap_or_else(|_| self.snapshot().generation)
    }

    /// Lease price polling for a symbol; polling stops when the returned lease drops.
    pub fn watch_price(&self, symbol: impl Into<String>) -> PriceLease {
        let symbol = symbol.into();
        {
            let mut d = self.shared.price_demand.lock().unwrap();
            *d.entry(symbol.clone()).or_insert(0) += 1;
        }
        let _ = self.shared.commands.send(Command::DemandChanged);
        PriceLease {
            shared: self.shared.clone(),
            symbol,
        }
    }

    /// Gracefully stop the reconciler and await its exit.
    pub async fn shutdown(self) {
        self.shared.cancel.cancel();
        let task = self.shared.task.lock().unwrap().take();
        if let Some(t) = task {
            let _ = t.await;
        }
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// An RAII lease keeping a symbol's price polled; dropping it releases the demand.
#[must_use = "dropping the lease stops price polling for the symbol"]
pub struct PriceLease {
    shared: Arc<Shared>,
    symbol: String,
}

impl std::fmt::Debug for PriceLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PriceLease")
            .field("symbol", &self.symbol)
            .finish()
    }
}

impl Drop for PriceLease {
    fn drop(&mut self) {
        let mut d = self.shared.price_demand.lock().unwrap();
        if let Some(c) = d.get_mut(&self.symbol) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                d.remove(&self.symbol);
            }
        }
        let _ = self.shared.commands.send(Command::DemandChanged);
    }
}

fn intent_of(req: &OrderCreateRequest) -> OptimisticOrder {
    use tossinvest_model::OrderCreateRequest as R;
    match req {
        R::QuantityBased(o) => OptimisticOrder {
            symbol: o.symbol.as_str().to_owned(),
            side: o.side,
            quantity: o.quantity.to_string(),
            price: o.price.as_ref().map(|p| p.to_string()),
            currency: tossinvest_model::Currency::Krw,
        },
        R::AmountBased(o) => OptimisticOrder {
            symbol: o.symbol.as_str().to_owned(),
            side: o.side,
            quantity: "0".to_owned(),
            price: None,
            currency: tossinvest_model::Currency::Usd,
        },
    }
}

// ── the pump ───────────────────────────────────────────────────────────────────────────

struct PumpCtx {
    store: Arc<Store>,
    account: AccountClient,
    commands: flume::Sender<Command>,
    rx: flume::Receiver<Command>,
    cancel: CancellationToken,
    config: SchedulerConfig,
    demand: Arc<Shared>,
}

async fn pump(ctx: PumpCtx) {
    let PumpCtx {
        store,
        account,
        commands,
        rx,
        cancel,
        config,
        demand,
    } = ctx;

    loop {
        let cadence = compute_cadence(&store.snapshot(), &config);
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                store.apply(|s| s.stop());
                break;
            }
            cmd = rx.recv_async() => {
                match cmd {
                    Ok(c) => handle_command(c, &store, &account, &commands, &demand),
                    Err(_) => break,
                }
            }
            _ = tokio::time::sleep(cadence) => {
                let _ = commands.send(Command::StartSweep);
            }
        }
    }
}

fn handle_command(
    cmd: Command,
    store: &Arc<Store>,
    account: &AccountClient,
    commands: &flume::Sender<Command>,
    demand: &Arc<Shared>,
) {
    match cmd {
        Command::StartSweep => {
            spawn_sweep(store, account, commands);
            spawn_prices(account, commands, demand);
        }
        Command::SweepDone { orders, dropped } => {
            for o in orders {
                store.apply(|s| s.observe_order(o));
            }
            for o in dropped {
                store.apply(|s| s.observe_order(o));
            }
        }
        Command::Observed(o) => store.apply(|s| s.observe_order(*o)),
        Command::Submit {
            local_id,
            intent,
            req,
        } => {
            store.apply(|s| {
                let (_k, d) = s.submit_optimistic(local_id, *intent);
                d
            });
            let account = account.clone();
            let commands = commands.clone();
            tokio::spawn(async move {
                let result = account
                    .create_order(&req)
                    .await
                    .map(|r| r.order_id)
                    .map_err(|e| e.to_string());
                let _ = commands.send(Command::SubmitResult { local_id, result });
            });
        }
        Command::SubmitResult { local_id, result } => match result {
            Ok(server_id) => store.apply(|s| s.confirm(local_id, server_id)),
            Err(msg) => store.apply(|s| s.submit_failed(OrderKey::Local(local_id), msg)),
        },
        Command::Cancel { server_id } => {
            store.apply(|s| s.op_submitted(&server_id, OpKind::Cancel));
            let account = account.clone();
            let commands = commands.clone();
            tokio::spawn(async move {
                if account.cancel_order(&server_id).await.is_ok() {
                    // Observe the original to watch PENDING_CANCEL → CANCELED / revert.
                    if let Ok(o) = account.get_order(&server_id).await {
                        let _ = commands.send(Command::Observed(Box::new(o)));
                    }
                }
            });
        }
        Command::Prices(prices) => {
            for p in prices {
                store.apply(|s| s.price_tick(p));
            }
        }
        Command::Refresh { target, reply } => {
            spawn_refresh(store, account, target, reply, commands.clone());
        }
        Command::DemandChanged | Command::Wake => { /* loop recomputes cadence from fresh snapshot */
        }
    }
}

fn spawn_sweep(store: &Arc<Store>, account: &AccountClient, commands: &flume::Sender<Command>) {
    let before = store.snapshot().open_server_ids();
    let account = account.clone();
    let commands = commands.clone();
    tokio::spawn(async move {
        let Ok(page) = account
            .orders(OrderListFilter::Open, None, None, None)
            .await
        else {
            return; // transient; try again next tick
        };
        let present: std::collections::HashSet<_> =
            page.orders.iter().map(|o| o.order_id.clone()).collect();
        let mut dropped = Vec::new();
        for id in before.difference(&present) {
            if let Ok(o) = account.get_order(id).await {
                dropped.push(o);
            }
        }
        let _ = commands.send(Command::SweepDone {
            orders: page.orders,
            dropped,
        });
    });
}

fn spawn_prices(account: &AccountClient, commands: &flume::Sender<Command>, demand: &Arc<Shared>) {
    let symbols: Vec<String> = {
        let d = demand.price_demand.lock().unwrap();
        d.keys().cloned().collect()
    };
    if symbols.is_empty() {
        return;
    }
    let client = account.client().clone();
    let commands = commands.clone();
    tokio::spawn(async move {
        let refs: Vec<&str> = symbols.iter().map(String::as_str).collect();
        if let Ok(prices) = client.prices(&refs).await {
            let _ = commands.send(Command::Prices(prices));
        }
    });
}

fn spawn_refresh(
    store: &Arc<Store>,
    account: &AccountClient,
    target: RefreshTarget,
    reply: oneshot::Sender<u64>,
    commands: flume::Sender<Command>,
) {
    let store = store.clone();
    let account = account.clone();
    tokio::spawn(async move {
        match target {
            RefreshTarget::Orders | RefreshTarget::All => {
                if let Ok(page) = account
                    .orders(OrderListFilter::Open, None, None, None)
                    .await
                {
                    for o in page.orders {
                        store.apply(|s| s.observe_order(o));
                    }
                }
                if matches!(target, RefreshTarget::All)
                    && let Ok(h) = account.holdings(None).await
                {
                    store.apply(|s| s.update_holdings(h));
                }
            }
            RefreshTarget::Holdings => {
                if let Ok(h) = account.holdings(None).await {
                    store.apply(|s| s.update_holdings(h));
                }
            }
            RefreshTarget::Order(id) => {
                if let Ok(o) = account.get_order(&id).await {
                    store.apply(|s| s.observe_order(o));
                }
            }
            RefreshTarget::Prices(symbols) => {
                let refs: Vec<&str> = symbols.iter().map(String::as_str).collect();
                if let Ok(prices) = account.client().prices(&refs).await {
                    for p in prices {
                        store.apply(|s| s.price_tick(p));
                    }
                }
            }
        }
        let _ = reply.send(store.snapshot().generation);
        let _ = commands.send(Command::Wake); // re-evaluate cadence now that state changed
    });
}

fn compute_cadence(snap: &ProjectedState, config: &SchedulerConfig) -> Duration {
    let mut hot = false;
    let mut working = false;
    for v in snap.orders.values() {
        if v.optimistic || matches!(v.lifecycle, crate::core::Lifecycle::Closing { .. }) {
            hot = true;
            break;
        }
        if v.is_working() {
            working = true;
        }
    }
    if hot {
        config.hot
    } else if working {
        config.working
    } else {
        config.idle
    }
}

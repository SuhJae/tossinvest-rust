//! A small CLI that prints a portfolio summary using the stateless client.
//!
//! Set credentials in the environment and run:
//!
//! ```bash
//! export TOSSINVEST_CLIENT_ID=...
//! export TOSSINVEST_CLIENT_SECRET=...
//! cargo run -p tossinvest --example portfolio            # account + holdings summary
//! cargo run -p tossinvest --example portfolio 005930 AAPL # also quote these symbols
//! ```

use tossinvest::{Credentials, TossClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let credentials = match Credentials::from_env() {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Set TOSSINVEST_CLIENT_ID and TOSSINVEST_CLIENT_SECRET in the environment.");
            std::process::exit(2);
        }
    };

    let client = TossClient::new(credentials)?;

    // Optional symbols to quote.
    let symbols: Vec<String> = std::env::args().skip(1).collect();
    if !symbols.is_empty() {
        let refs: Vec<&str> = symbols.iter().map(String::as_str).collect();
        println!("Quotes");
        println!("------");
        match client.prices(&refs).await {
            Ok(prices) => {
                for p in prices {
                    println!("  {:<8} {} {}", p.symbol, p.last_price, p.currency);
                }
            }
            Err(e) => eprintln!("  failed to fetch prices: {e}"),
        }
        println!();
    }

    // Accounts.
    let accounts = client.accounts().await?;
    if accounts.is_empty() {
        println!("No accounts found.");
        return Ok(());
    }
    println!("Accounts");
    println!("--------");
    for a in &accounts {
        println!(
            "  seq={} no={} type={}",
            a.account_seq, a.account_no, a.account_type
        );
    }
    println!();

    // Holdings for the first account.
    let acct = client.account(accounts[0].account_seq);
    let holdings = acct.holdings(None).await?;
    println!("Holdings (account {})", accounts[0].account_seq);
    println!("--------");
    if holdings.items.is_empty() {
        println!("  (no holdings)");
    } else {
        for h in &holdings.items {
            println!(
                "  {:<8} {:<14} qty={:<8} last={:<10} P/L={} ({}%)",
                h.symbol,
                h.name,
                h.quantity,
                h.last_price,
                h.profit_loss.amount,
                // ratio → percent for display
                h.profit_loss.rate.get() * rust_decimal::Decimal::from(100),
            );
        }
    }
    let mv = &holdings.market_value.amount;
    print!("  total market value: KRW {}", mv.krw);
    if let Some(usd) = mv.usd {
        print!(" / USD {usd}");
    }
    println!();

    Ok(())
}

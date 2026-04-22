use jian_guo_yun_relay::bootstrap;

#[tokio::main]
async fn main() {
    if let Err(e) = bootstrap::run().await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[tokio::main]
async fn main() {
    if std::env::args()
        .skip(1)
        .any(|a| a == server::healthcheck::FLAG)
    {
        std::process::exit(server::healthcheck::run());
    }
    if let Err(err) = server::run().await {
        eprintln!("fatal: {err}");
        std::process::exit(1);
    }
}

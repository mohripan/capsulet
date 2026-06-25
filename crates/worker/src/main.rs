#[tokio::main]
async fn main() -> anyhow::Result<()> {
    capsulet_worker::runtime::run().await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    capsulet_scheduler::run().await
}

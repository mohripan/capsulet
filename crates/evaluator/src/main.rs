#[tokio::main]
async fn main() -> anyhow::Result<()> {
    capsulet_evaluator::runtime::run().await
}

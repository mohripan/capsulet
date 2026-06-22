#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    capsulet_evaluator::runtime::run().await
}

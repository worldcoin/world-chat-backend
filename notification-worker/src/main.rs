mod types;

use types::Environment;

#[tokio::main]
async fn main() {
    let env = Environment::from_env();
    println!("Hello from XMTP Notification Worker!");
    println!("Running in {:?} environment", env);
}

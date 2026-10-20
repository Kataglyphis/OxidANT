#[flutter_rust_bridge::frb(sync)]
pub fn greet(name: String) -> String {
    format!("Hello, you {name}!")
}

/// Forwarding shim; the real implementation lives in `crate::platform` (wasm/native).
pub fn init_app() {
    crate::platform::init_app();
}

/// Synchronous heavy computation demo (template stub).
#[flutter_rust_bridge::frb(sync)]
pub fn heavy_computation(input: i32) -> i32 {
    let mut result = input;
    for _ in 0..1000 {
        result = (result * 2) % 1000000;
    }
    result
}

/// Asynchronous work demo (template stub).
#[flutter_rust_bridge::frb()]
pub async fn async_heavy_work(input: i32) -> i32 {
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    input * 2
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_demo_calls_answer() {
        assert_eq!(super::greet("Ant".into()), "Hello, you Ant!");
        // 2^1000 mod 10^6, by repeated doubling.
        assert_eq!(super::heavy_computation(1), 69_376);
    }
}

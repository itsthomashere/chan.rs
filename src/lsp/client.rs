use async_trait::async_trait;

#[async_trait]
trait LspClient {
    async fn send_request();
    async fn progress();
    async fn register_capabilities();
    async fn unregister_capabilities();
    async fn update_capabilities();
}

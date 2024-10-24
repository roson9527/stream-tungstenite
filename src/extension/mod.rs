mod status_viewer;

pub mod interface {
    use crate::prelude::{ShareListener, WsStreamStatus};
    use async_trait::async_trait;
    use eyre::Result as EResult;
    use std::sync::Arc;
    use tungstenite::Message;

    #[async_trait]
    pub trait ReconnectTExtension {
        async fn init_msg_stream(&self, msg_listener: Arc<ShareListener<Message>>) -> EResult<()>;

        async fn init_status_stream(
            &self,
            status_listener: Arc<ShareListener<WsStreamStatus>>,
        ) -> EResult<()>;
    }
}

pub mod prelude {
    pub use crate::extension::interface::*;
    pub use crate::extension::status_viewer::*;
}

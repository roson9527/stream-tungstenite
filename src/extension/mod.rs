mod status_viewer;

pub mod interface {
    use crate::prelude::WsStreamStatus;
    use async_trait::async_trait;
    use eyre::Result as EResult;
    use std::sync::Arc;
    use tokio_stream::wrappers::UnboundedReceiverStream;
    use tungstenite::Message;

    #[async_trait]
    pub trait ReconnectTMsgExtension {
        async fn init_msg_stream(
            &self,
            msg_stream: UnboundedReceiverStream<Message>,
        ) -> EResult<()>;
    }

    #[async_trait]
    pub trait ReconnectTStatusExtension {
        async fn init_status_stream(
            &self,
            status_stream: UnboundedReceiverStream<WsStreamStatus>,
        ) -> EResult<()>;
    }

    #[async_trait]
    pub trait ReconnectTAllExtension: ReconnectTMsgExtension + ReconnectTStatusExtension {}

    pub enum ExtensionType {
        Msg(Arc<dyn ReconnectTMsgExtension>),
        Status(Arc<dyn ReconnectTStatusExtension>),
        All(Arc<dyn ReconnectTAllExtension>),
    }
}

pub use crate::extension::interface::*;
pub use crate::extension::status_viewer::*;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("剪贴板错误: {0}")]
    Clipboard(String),

    #[error("网络错误: {0}")]
    Network(#[from] std::io::Error),

    #[error("设备发现错误: {0}")]
    Discovery(String),

    #[error("协议错误: {0}")]
    Protocol(String),

    #[error("序列化错误: {0}")]
    Serialization(#[from] bincode::Error),

    #[error("图像处理错误: {0}")]
    Image(#[from] image::ImageError),

    #[error("文件传输错误: {0}")]
    Transfer(String),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("校验和不匹配: 期望 {expected}, 实际 {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("无连接设备")]
    NoConnection,

    #[error("通道发送失败")]
    ChannelSend,
}

pub type AppResult<T> = Result<T, AppError>;

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for AppError {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        AppError::ChannelSend
    }
}

impl<T> From<tokio::sync::broadcast::error::SendError<T>> for AppError {
    fn from(_: tokio::sync::broadcast::error::SendError<T>) -> Self {
        AppError::ChannelSend
    }
}

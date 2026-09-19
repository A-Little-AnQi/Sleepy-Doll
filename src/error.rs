use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("配置错误：{0}")]
    Config(String),
    #[error("I/O 错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 错误：{0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP 错误：{0}")]
    Http(String),
    #[error("请求超时：{0}")]
    Timeout(String),
    #[error("模型协议错误：{0}")]
    ModelProtocol(String),
    #[error("工具错误：{0}")]
    Tool(String),
    #[error("存储错误：{0}")]
    Storage(#[from] rusqlite::Error),
    #[error("操作已取消")]
    Cancelled,
    #[error("运行时冲突：{0}")]
    Conflict(String),
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        if value.is_timeout() {
            Self::Timeout("等待服务响应超时，请检查连接或调整响应超时设置。".into())
        } else {
            Self::Http(value.without_url().to_string())
        }
    }
}

impl From<ureq::Error> for Error {
    fn from(value: ureq::Error) -> Self {
        Self::Http(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn user_message(&self) -> String {
        match self {
            Self::ModelProtocol(_) => "模型响应没有完整结束，未执行这轮工具调用。".into(),
            Self::Http(message) if message.starts_with("模型") => message.clone(),
            Self::Http(_) => "连接中断或服务未响应，请稍后核对运行结果。".into(),
            Self::Timeout(message) => message.clone(),
            Self::Storage(_) => "本地记录保存失败，已停止派发新动作。".into(),
            Self::Cancelled => "已停止。".into(),
            Self::Io(_) => "本地文件或插件进程无法访问，请检查路径与权限。".into(),
            Self::Json(_) => "收到的数据格式不完整或不正确。".into(),
            Self::Config(s) | Self::Tool(s) | Self::Conflict(s) => {
                if s.is_ascii() {
                    "当前操作无法继续，请检查配置或核对执行记录。".into()
                } else {
                    s.clone()
                }
            }
        }
    }
}

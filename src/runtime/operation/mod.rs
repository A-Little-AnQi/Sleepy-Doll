//! 事务操作：一次改动从计划、授权、执行到验证与回滚。

pub mod kernel;
pub mod operations;
pub mod permissions;
pub mod verifier;
pub mod workflow;

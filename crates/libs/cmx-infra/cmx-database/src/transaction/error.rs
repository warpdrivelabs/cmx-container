/*
 * @Author: yqs
 * @Date: 2026-03-05 19:28:02
 * @Describe: 事务错误类型定义
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-09 15:00:00
 */
use derive_more::From;
use serde::Serialize;
use serde_with::{serde_as, DisplayFromStr};

pub type Result<T> = core::result::Result<T, Error>;

#[serde_as]
#[derive(Debug, Serialize, From)]
pub enum Error {
	/// 无法提交事务，因为没有打开的事务
	TxnCantCommitNoOpenTxn,
	/// 无法开始事务，因为 with_txn 为 false
	CannotBeginTxnWithTxnFalse,
	/// 无法提交事务，因为 with_txn 为 false
	CannotCommitTxnWithTxnFalse,
	/// 没有活跃事务
	NoTxn,
	/// 数据库未找到
	DbNotFound(String),
	/// 无法创建模型管理器提供者
	CantCreateModelManagerProvider(String),
	/// 连接超时
	ConnectionTimeout,
	/// 连接池耗尽
	PoolExhausted,
	/// 需要事务
	TransactionRequired,
	/// 不允许事务
	TransactionNotAllowed,
	/// 不支持的数据库类型
	UnsupportedDbType,
	/// 数据库不存在
	NoDb,
	
	// -- Externals
	/// SQLx 错误
	#[from]
	Sqlx(#[serde_as(as = "DisplayFromStr")] sqlx::Error),
}

// region:    --- Error Boilerplate

impl core::fmt::Display for Error {
	fn fmt(
		&self,
		fmt: &mut core::fmt::Formatter,
	) -> core::result::Result<(), core::fmt::Error> {
		write!(fmt, "{self:?}")
	}
}

impl std::error::Error for Error {}

// endregion: --- Error Boilerplate
/*
 * @Author: yqs
 * @Date: 2026-03-05 19:28:02
 * @Describe: 
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-05 20:46:02
 */
use derive_more::From;
use serde::Serialize;
use serde_with::{serde_as, DisplayFromStr};

pub type Result<T> = core::result::Result<T, Error>;

#[serde_as]
#[derive(Debug, Serialize, From)]
pub enum Error {
	TxnCantCommitNoOpenTxn,
	CannotBeginTxnWithTxnFalse,
	CannotCommitTxnWithTxnFalse,
	NoTxn,
	DbNotFound(String),
	CantCreateModelManagerProvider(String),
	ConnectionTimeout,
	PoolExhausted,
	TransactionRequired,
	TransactionNotAllowed,
	
	// -- Externals
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

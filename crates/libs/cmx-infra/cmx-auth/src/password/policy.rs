//! 密码策略校验。
//!
//! 校验新密码是否满足强度要求（长度、字符类型组合等）。

use cmx_traits::auth::AuthError;

/// 密码策略校验器。
///
/// 校验新密码是否满足强度要求（长度、字符类型组合等），
/// 策略当前为硬编码默认值，未来可通过配置扩展。
pub struct PasswordPolicy {
    /// 密码最小长度。
    min_length: usize,

    /// 是否要求至少 1 个大写字母。
    require_uppercase: bool,

    /// 是否要求至少 1 个小写字母。
    require_lowercase: bool,

    /// 是否要求至少 1 个数字。
    require_digit: bool,

    /// 是否要求至少 1 个特殊字符（`!@#$%^&*()_+-=[]{}|;':",./<>?\`~`）。
    require_special: bool,
}

impl PasswordPolicy {
    /// 创建默认的密码策略校验器。
    ///
    /// 默认策略：最小长度 8 位，必须包含大写字母、小写字母、数字和特殊字符。
    ///
    /// # Returns
    ///
    /// 返回使用默认策略的 `PasswordPolicy` 实例。
    pub fn new() -> Self {
        Self {
            min_length: 8,
            require_uppercase: true,
            require_lowercase: true,
            require_digit: true,
            require_special: true,
        }
    }

    /// 校验密码是否符合策略。
    ///
    /// 依次检查长度、大写字母、小写字母、数字、特殊字符要求，
    /// 任一不满足时收集错误信息并返回 `AuthError::PasswordPolicyViolated`。
    ///
    /// # Arguments
    ///
    /// * `password` - 待校验的明文密码。
    ///
    /// # Returns
    ///
    /// 符合策略时返回 `Ok(())`，否则返回 `AuthError::PasswordPolicyViolated`（含所有违规描述）。
    pub fn validate(&self, password: &str) -> Result<(), AuthError> {
        let mut errors = Vec::new();

        if password.len() < self.min_length {
            errors.push(format!("密码长度不能少于 {} 位", self.min_length));
        }
        if self.require_uppercase && !password.chars().any(|c| c.is_ascii_uppercase()) {
            errors.push("密码必须包含大写字母".to_string());
        }
        if self.require_lowercase && !password.chars().any(|c| c.is_ascii_lowercase()) {
            errors.push("密码必须包含小写字母".to_string());
        }
        if self.require_digit && !password.chars().any(|c| c.is_ascii_digit()) {
            errors.push("密码必须包含数字".to_string());
        }
        if self.require_special
            && !password
                .chars()
                .any(|c| "!@#$%^&*()_+-=[]{}|;':\",./<>?`~".contains(c))
        {
            errors.push("密码必须包含特殊字符".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(AuthError::PasswordPolicyViolated(errors.join("; ")))
        }
    }
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self::new()
    }
}

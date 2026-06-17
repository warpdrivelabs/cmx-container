//! 密码策略校验

use cmx_traits::auth::AuthError;

/// 密码策略校验器
pub struct PasswordPolicy {
    min_length: usize,
    require_uppercase: bool,
    require_lowercase: bool,
    require_digit: bool,
    require_special: bool,
}

impl PasswordPolicy {
    pub fn new() -> Self {
        Self {
            min_length: 8,
            require_uppercase: true,
            require_lowercase: true,
            require_digit: true,
            require_special: true,
        }
    }

    /// 校验密码是否符合策略
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

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

#[cfg(test)]
mod tests {
    use super::*;
    use cmx_traits::auth::AuthError;

    #[test]
    fn test_password_policy_valid_password() {
        // 满足所有要求的密码：长度>=8 + 大写 + 小写 + 数字 + 特殊字符
        let policy = PasswordPolicy::new();

        // 大小写、数字、特殊字符齐全
        assert!(policy.validate("Abcd1234!").is_ok());
        assert!(policy.validate("P@ssw0rd").is_ok());
        assert!(policy.validate("Zx1!abcd").is_ok());
    }

    #[test]
    fn test_password_policy_too_short() {
        let policy = PasswordPolicy::new();

        // 长度不足 8 位
        let result = policy.validate("Ab1!");
        assert!(result.is_err(), "过短密码应违反策略");
        if let Err(AuthError::PasswordPolicyViolated(msg)) = result {
            assert!(msg.contains("长度"), "错误信息应包含 '长度'，实际: {}", msg);
        } else {
            panic!("期望 PasswordPolicyViolated 错误");
        }
    }

    #[test]
    fn test_password_policy_missing_uppercase() {
        let policy = PasswordPolicy::new();

        // 缺少大写字母
        let result = policy.validate("abcd1234!");
        assert!(result.is_err(), "缺少大写字母应违反策略");
        if let Err(AuthError::PasswordPolicyViolated(msg)) = result {
            assert!(
                msg.contains("大写字母"),
                "错误信息应包含 '大写字母'，实际: {}",
                msg
            );
        }
    }

    #[test]
    fn test_password_policy_missing_lowercase() {
        let policy = PasswordPolicy::new();

        // 缺少小写字母
        let result = policy.validate("ABCD1234!");
        assert!(result.is_err(), "缺少小写字母应违反策略");
        if let Err(AuthError::PasswordPolicyViolated(msg)) = result {
            assert!(
                msg.contains("小写字母"),
                "错误信息应包含 '小写字母'，实际: {}",
                msg
            );
        }
    }

    #[test]
    fn test_password_policy_missing_digit() {
        let policy = PasswordPolicy::new();

        // 缺少数字
        let result = policy.validate("Abcdefgh!");
        assert!(result.is_err(), "缺少数字应违反策略");
        if let Err(AuthError::PasswordPolicyViolated(msg)) = result {
            assert!(msg.contains("数字"), "错误信息应包含 '数字'，实际: {}", msg);
        }
    }

    #[test]
    fn test_password_policy_missing_special() {
        let policy = PasswordPolicy::new();

        // 缺少特殊字符
        let result = policy.validate("Abcd1234");
        assert!(result.is_err(), "缺少特殊字符应违反策略");
        if let Err(AuthError::PasswordPolicyViolated(msg)) = result {
            assert!(
                msg.contains("特殊字符"),
                "错误信息应包含 '特殊字符'，实际: {}",
                msg
            );
        }
    }

    #[test]
    fn test_password_policy_multiple_violations_collected() {
        let policy = PasswordPolicy::new();

        // 同时违反多个规则：过短 + 缺大写 + 缺数字 + 缺特殊字符
        let result = policy.validate("abc");
        assert!(result.is_err(), "多重违规应返回错误");
        if let Err(AuthError::PasswordPolicyViolated(msg)) = result {
            // 错误信息应包含所有违规项（用分号分隔）
            assert!(msg.contains("长度"), "应包含长度违规: {}", msg);
            assert!(msg.contains("大写字母"), "应包含大写字母违规: {}", msg);
            assert!(msg.contains("数字"), "应包含数字违规: {}", msg);
            assert!(msg.contains("特殊字符"), "应包含特殊字符违规: {}", msg);
            // 不应包含小写字母违规（密码含小写字母）
            assert!(!msg.contains("小写字母"), "不应包含小写字母违规: {}", msg);
        }
    }

    #[test]
    fn test_password_policy_all_special_chars_accepted() {
        // 测试策略中定义的所有特殊字符
        let policy = PasswordPolicy::new();

        // `!@#$%^&*()_+-=[]{}|;':",./<>?`~` 中的每个字符都应被接受
        let specials = "!@#$%^&*()_+-=[]{}|;':\",./<>?`~";
        for ch in specials.chars() {
            let password = format!("Abc1234{}", ch);
            let result = policy.validate(&password);
            assert!(
                result.is_ok(),
                "包含特殊字符 '{}' 的密码应通过: {}",
                ch,
                password
            );
        }
    }

    #[test]
    fn test_password_policy_empty_password() {
        let policy = PasswordPolicy::new();

        let result = policy.validate("");
        assert!(result.is_err(), "空密码应违反多项规则");
        if let Err(AuthError::PasswordPolicyViolated(msg)) = result {
            // 空密码应触发所有规则违规
            assert!(msg.contains("长度"));
            assert!(msg.contains("大写字母"));
            assert!(msg.contains("小写字母"));
            assert!(msg.contains("数字"));
            assert!(msg.contains("特殊字符"));
        }
    }
}

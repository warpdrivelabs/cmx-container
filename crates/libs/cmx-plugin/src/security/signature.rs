//! 签名验证模块
//! 
//! 验证插件签名，确保插件来源可信

use std::path::Path;
use std::io::Read;

use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};

/// 签名验证器
/// 
/// 使用 Ed25519 算法验证插件签名。
pub struct SignatureValidator {
    /// 受信任的公钥列表
    trusted_public_keys: Vec<VerifyingKey>,
}

impl SignatureValidator {
    /// 创建新的签名验证器
    pub fn new() -> Self {
        Self {
            trusted_public_keys: Vec::new(),
        }
    }
    
    /// 添加受信任的公钥
    /// 
    /// 公钥应为 Base64 编码的 Ed25519 公钥。
    pub fn add_public_key(&mut self, public_key: &str) -> Result<(), String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(public_key)
            .map_err(|e| format!("解码公钥失败: {}", e))?;
        
        let verifying_key = VerifyingKey::from_bytes(
            bytes.as_slice().try_into()
                .map_err(|_| "公钥长度无效，应为32字节".to_string())?
        ).map_err(|e| format!("解析公钥失败: {}", e))?;
        
        self.trusted_public_keys.push(verifying_key);
        Ok(())
    }
    
    /// 添加原始公钥字节
    pub fn add_public_key_bytes(&mut self, bytes: [u8; 32]) -> Result<(), String> {
        let verifying_key = VerifyingKey::from_bytes(&bytes)
            .map_err(|e| format!("解析公钥失败: {}", e))?;
        self.trusted_public_keys.push(verifying_key);
        Ok(())
    }
    
    /// 验证签名
    /// 
    /// 使用受信任的公钥验证数据和签名。
    pub fn verify(&self, data: &[u8], signature: &[u8]) -> Result<bool, String> {
        if self.trusted_public_keys.is_empty() {
            return Err("没有配置受信任的公钥".to_string());
        }
        
        let sig = Signature::from_slice(signature)
            .map_err(|e| format!("解析签名失败: {}", e))?;
        
        // 尝试使用每个受信任的公钥验证
        for public_key in &self.trusted_public_keys {
            if public_key.verify_strict(data, &sig).is_ok() {
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    /// 验证文件签名
    /// 
    /// 读取文件内容并验证签名。
    pub fn verify_file(&self, file_path: &Path, signature: &[u8]) -> Result<bool, String> {
        let mut file = std::fs::File::open(file_path)
            .map_err(|e| format!("打开文件失败: {}", e))?;
        
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|e| format!("读取文件失败: {}", e))?;
        
        self.verify(&buffer, signature)
    }
    
    /// 验证 Base64 编码的签名
    /// 
    /// 接受 Base64 编码的签名字符串。
    pub fn verify_base64(&self, data: &[u8], signature_b64: &str) -> Result<bool, String> {
        let signature = base64::engine::general_purpose::STANDARD
            .decode(signature_b64)
            .map_err(|e| format!("解码签名失败: {}", e))?;
        
        self.verify(data, &signature)
    }
    
    /// 验证文件签名（Base64 编码）
    pub fn verify_file_base64(&self, file_path: &Path, signature_b64: &str) -> Result<bool, String> {
        let mut file = std::fs::File::open(file_path)
            .map_err(|e| format!("打开文件失败: {}", e))?;
        
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|e| format!("读取文件失败: {}", e))?;
        
        self.verify_base64(&buffer, signature_b64)
    }
    
    /// 获取受信任公钥数量
    pub fn trusted_key_count(&self) -> usize {
        self.trusted_public_keys.len()
    }
    
    /// 清空受信任公钥
    pub fn clear_keys(&mut self) {
        self.trusted_public_keys.clear();
    }
}

impl Default for SignatureValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// 签名信息
#[derive(Debug, Clone)]
pub struct SignatureInfo {
    /// 签名算法
    pub algorithm: String,
    /// 签名者密钥ID
    pub key_id: String,
    /// 签名值（Base64）
    pub signature: String,
}

impl SignatureInfo {
    /// 创建新的签名信息
    pub fn new(algorithm: String, key_id: String, signature: String) -> Self {
        Self { algorithm, key_id, signature }
    }
    
    /// 从 JSON 解析签名信息
    pub fn from_json(json: &serde_json::Value) -> Result<Self, String> {
        let algorithm = json.get("algorithm")
            .and_then(|v| v.as_str())
            .unwrap_or("Ed25519")
            .to_string();
        
        let key_id = json.get("key_id")
            .and_then(|v| v.as_str())
            .ok_or("缺少 key_id 字段")?
            .to_string();
        
        let signature = json.get("signature")
            .and_then(|v| v.as_str())
            .ok_or("缺少 signature 字段")?
            .to_string();
        
        Ok(Self { algorithm, key_id, signature })
    }
}

# CMX Plugin 集群功能需求文档

## 1. 背景与目标

### 1.1 背景
服务采用多节点部署架构，需要确保各节点插件状态一致。

### 1.2 目标
1. 当一个节点执行插件安装、升级、降级等操作时，其他节点同步执行相同操作
2. 新节点上线时，自动同步已安装的插件列表和版本信息
3. **关键约束**：插件安装或升级时的 SQL 脚本只能执行一次，避免多节点重复执行

---

## 2. 核心问题

### 2.1 SQL 执行唯一性
- 插件**安装**时可能有建表 SQL
- 插件**升级**时可能有数据迁移 SQL
- **同一个版本的插件的 SQL 只能执行一次**
- 需要记录 SQL 是否已执行成功

### 2.2 示例场景

```
场景1：插件安装
- Node A 安装 plugin-x v1.0.0
- Node A 执行 SQL（建表）
- Node A 发布安装消息
- Node B、C 收到消息，安装 plugin-x v1.0.0（跳过 SQL）

场景2：插件升级
- Node A 升级 plugin-x 从 v1.0.0 到 v1.1.0
- Node A 执行 SQL（数据迁移）
- Node A 发布升级消息
- Node B、C 收到消息，升级 plugin-x 到 v1.1.0（跳过 SQL）

场景3：新节点上线
- Node D 新上线
- 从 Redis/DB 同步插件列表
- 安装 plugin-x v1.1.0（跳过 SQL，因为 SQL 已执行）
```

---

## 3. 功能需求

### 3.1 插件注册表（Plugin Registry）

**存储位置**：**数据库**（持久化存储）

**数据表设计**：
```sql
-- 插件注册表
CREATE TABLE cmx_plugin_registry (
    plugin_id VARCHAR(128) PRIMARY KEY,
    version VARCHAR(64) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'installed',
    installed_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    installed_by VARCHAR(128),  -- 安装节点ID
    metadata JSON               -- 额外元数据
);

-- SQL 执行记录
CREATE TABLE cmx_plugin_sql_record (
    id BIGINT PRIMARY KEY AUTO_INCREMENT,
    plugin_id VARCHAR(128) NOT NULL,
    version VARCHAR(64) NOT NULL,       -- SQL 对应的版本
    sql_type VARCHAR(32) NOT NULL,      -- install/upgrade/downgrade
    status VARCHAR(32) NOT NULL,        -- pending/executing/success/failed
    executed_at TIMESTAMP,
    executed_by VARCHAR(128),           -- 执行节点ID
    error_message TEXT,                 -- 错误信息
    rollback_sql TEXT,                  -- 回滚 SQL
    UNIQUE KEY uk_plugin_version (plugin_id, version)
);
```

**Redis 用途**：
```
# 集群消息通道
Key: cmx:plugins:cluster:operations
Type: Pub/Sub Channel

# 分布式锁
Key: cmx:plugin:lock:{plugin_id}
Type: Lock
TTL: 60s

# 操作幂等性检查（可选，短期缓存）
Key: cmx:plugin:operation:{operation_id}
Type: String
TTL: 300s
```

**API**：
- `register_plugin(plugin_id, version, node_id)` - 注册插件到数据库
- `unregister_plugin(plugin_id)` - 从数据库注销插件
- `get_plugin_info(plugin_id)` - 从数据库获取插件信息
- `get_all_plugins()` - 从数据库获取所有已注册插件
- `mark_sql_executed(plugin_id, version, sql_type, node_id)` - 标记 SQL 已执行
- `is_sql_executed(plugin_id, version)` - 检查 SQL 是否已执行成功
- `get_sql_status(plugin_id, version)` - 获取 SQL 执行状态

### 3.2 集群操作消息协议

**消息通道**：`cmx:plugins:cluster:operations`

**消息结构**：
```rust
pub struct ClusterOperationMessage {
    /// 操作ID（UUID，用于幂等性检查）
    pub operation_id: String,
    /// 操作类型
    pub operation_type: ClusterOperationType,
    /// 插件ID
    pub plugin_id: String,
    /// 插件版本
    pub version: String,
    /// 旧版本（升级/降级时使用）
    pub old_version: Option<String>,
    /// 操作来源节点
    pub source_node: String,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// SQL 是否已执行
    pub sql_executed: bool,
    /// 已执行的 SQL 版本列表
    pub sql_versions: Vec<String>,
}

pub enum ClusterOperationType {
    Install,    // 安装
    Upgrade,    // 升级
    Downgrade,  // 降级
    Uninstall,  // 卸载
    Activate,   // 激活
    Deactivate, // 停用
}
```

### 3.3 分布式操作协调器

**功能**：
1. 获取分布式锁（Redis）
2. 检查数据库：SQL 是否已执行
3. 执行 SQL（如果未执行）
4. 记录 SQL 执行状态到数据库
5. 执行本地安装/升级操作
6. 保存插件记录到数据库
7. 发布操作消息（Redis Pub/Sub）

**流程**：
```
┌─────────────────────────────────────────────────────────────┐
│                    完整操作流程（主节点）                      │
├─────────────────────────────────────────────────────────────┤
│  1. 获取分布式锁 (Redis: cmx:plugin:lock:{plugin_id})        │
│  2. 查询数据库：该版本 SQL 是否已执行成功？                    │
│     ├─ 成功 → 跳过 SQL                                       │
│     ├─ 失败 → 报错，不允许继续                                │
│     └─ 未执行 → 执行 SQL                                     │
│         ├─ 成功 → 记录到数据库 (status=success)              │
│         └─ 失败 → 回滚，记录错误 (status=failed)，报错        │
│  3. 执行本地安装/升级操作                                     │
│  4. 保存插件记录到数据库                                      │
│  5. 发布操作消息到 Redis Pub/Sub                             │
│  6. 释放锁                                                   │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    本地操作流程（从节点）                      │
├─────────────────────────────────────────────────────────────┤
│  1. 收到操作消息                                              │
│  2. 检查消息幂等性（operation_id）                             │
│  3. 查询数据库：确认该版本 SQL 是否已执行成功                   │
│     ├─ 成功 → 继续安装                                        │
│     ├─ 失败 → 记录日志，不执行安装                             │
│     └─ 未执行 → 等待主节点完成或报错                           │
│  4. 执行本地安装/升级操作（跳过 SQL）                          │
│  5. 更新本地状态                                              │
└─────────────────────────────────────────────────────────────┘
```

### 3.4 消息订阅监听器

**功能**：
1. 订阅集群操作通道
2. 接收并解析操作消息
3. 执行本地操作（跳过 SQL）
4. 处理幂等性

### 3.5 新节点初始化

**流程**：
```
1. 从 Redis 获取插件注册表
2. 对比本地已安装插件
3. 安装缺失的插件（跳过 SQL）
4. 注册当前节点到集群
5. 启动消息监听器
```

---

## 4. 需要完善的功能清单

| 序号 | 功能模块 | 优先级 | 文件 | 说明 |
|------|----------|--------|------|------|
| 1 | PluginRegistry | P0 | cluster/registry.rs | Redis 插件注册表 |
| 2 | ClusterOperationMessage | P0 | cluster/message.rs | 操作消息协议 |
| 3 | OperationCoordinator | P0 | cluster/coordinator.rs | 分布式操作协调 |
| 4 | ClusterMessageListener | P0 | cluster/listener.rs | 消息订阅监听 |
| 5 | install_local_only | P0 | service/install.rs | 本地安装（跳过SQL） |
| 6 | upgrade_local_only | P0 | service/upgrade.rs | 本地升级（跳过SQL） |
| 7 | initialize_new_node | P1 | core/manager.rs | 新节点初始化 |
| 8 | 节点心跳+租约 | P2 | cluster/node.rs | 自动检测节点下线 |

---

## 5. 技术要点

### 5.1 SQL 执行状态存储
- 使用 Redis Set 存储已执行 SQL 的版本列表
- Key: `cmx:plugins:sql:{plugin_id}`
- 每执行一个版本的 SQL，将版本号加入 Set

### 5.2 幂等性保证
- 每个操作生成唯一 operation_id（UUID）
- 使用 Redis 记录已处理的 operation_id
- 处理消息前先检查是否已处理

### 5.3 分布式锁
- 使用 cmx-buffer 的 LockManager
- 锁 Key: `cmx:plugin:lock:{plugin_id}`
- 锁超时时间: 60秒（可配置）

### 5.4 消息可靠性
- 使用 Redis Pub/Sub 发布消息
- 新节点上线时主动请求全量同步
- 操作失败时记录日志，支持手动修复

---

## 6. 配置项

```rust
pub struct ClusterSettings {
    /// 当前节点ID
    pub node_id: String,
    /// 心跳间隔（秒）
    pub heartbeat_interval_seconds: u64,
    /// 心跳超时（秒）
    pub heartbeat_timeout_seconds: i64,
    /// 是否启用集群模式
    pub enabled: bool,
    /// 操作通道名称
    pub operation_channel: String,
    /// 分布式锁超时（秒）
    pub lock_timeout_seconds: u64,
}
```

---

## 7. 已确认问题

1. ~~插件升级时是否也有 SQL 操作？~~ **已确认：有**
2. ~~SQL 执行失败时如何处理？~~ **已确认：回滚并记录错误**
3. ~~跨版本升级如何处理？~~ **已确认：**
   - 检测是否安装过中间版本
   - 没有安装过中间版本：可以直接升级
   - 安装过中间版本但 SQL 执行失败：不允许升级，提示错误
4. ~~节点下线后重新上线如何处理？~~ **已确认：全量对比同步**
   - 从 Redis 获取插件注册表
   - 对比本地已安装插件
   - 安装/升级缺失的插件（跳过 SQL）

---

*文档创建时间: 2026-03-20*
*最后更新: 2026-03-20*

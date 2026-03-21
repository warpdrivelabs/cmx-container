### 心跳机制现状
NodeManager 中 有心跳机制的设计 ，但 没有自动执行 ：

功能 状态 说明 last_heartbeat 字段 ✅ 已实现 记录最后心跳时间 update_heartbeat() ✅ 已实现 手动更新心跳 is_healthy() ✅ 已实现 检查节点是否健康（默认30秒超时） check_and_update_health() ✅ 已实现 检查并标记不健康节点 自动心跳发送 ❌ 未实现 没有后台任务定时发送心跳 Redis 存储节点状态 ❌ 未实现 节点状态只存在内存中

### 3. 服务异常关闭自动下线
当前没有实现自动下线机制 ：

- 节点状态存储在本地内存 HashMap<String, NodeInfo> 中
- 没有持久化到 Redis
- 没有实现"租约"机制（Redis key 过期自动删除）
- 服务异常关闭时，其他节点无法感知
### 4. 状态同步机制
SyncManager 设计了基于 Redis Pub/Sub 的同步机制：

```
// 同步通道
sync_channel: "cmx:plugin:sync"

// 消息类型
- StateUpdate    // 状态更新
- StateDelete    // 状态删除
- FullSyncRequest  // 全量同步请求
- FullSyncResponse // 全量同步响应
```
但同样 没有被 PluginManager 调用 。

## 🔧 如果需要完整的集群功能，需要补充以下内容：
1. 节点注册到 Redis ：启动时将节点信息写入 Redis
2. 心跳后台任务 ：定时更新 Redis 中的节点心跳
3. 租约机制 ：使用 Redis key 过期实现自动下线
4. 健康检查任务 ：定时检查其他节点状态
5. 插件状态同步 ：在安装/激活/卸载时调用 SyncManager
## Cluster 模块功能详解
cluster 模块包含三个子模块，分别负责节点管理、状态同步和部署协调：

### 1️⃣ NodeManager（节点管理模块）
文件位置 : cluster/node.rs
数据结构
```
/// 节点状态
pub enum NodeStatus {
    Online,       // 在线
    Offline,      // 离线
    Maintenance,  // 维护中
}

/// 节点信息
pub struct NodeInfo {
    pub id: String,                    // 节点ID
    pub name: String,                  // 节点名称
    pub address: String,               // 节点地址
    pub status: NodeStatus,            // 节点状态
    pub last_heartbeat: DateTime<Utc>, // 最后心跳时间
    pub metadata: HashMap<String, String>, // 元数据（如 
    active_plugins）
}

/// 配置
pub struct NodeManagerConfig {
    pub heartbeat_timeout_seconds: i64,      // 心跳超时（默认30
    秒）
    pub health_check_interval_seconds: u64,  // 健康检查间隔（默认
    10秒）
    pub enable_distributed_lock: bool,       // 是否启用分布式锁
}
``` 主要功能
方法 功能 状态 register_node() 注册节点到本地列表 ✅ 已实现 unregister_node() 从本地列表注销节点 ✅ 已实现 get_node() / get_all_nodes() 获取节点信息 ✅ 已实现 get_online_nodes() 获取所有在线节点 ✅ 已实现 get_healthy_nodes() 获取健康节点（心跳未超时） ✅ 已实现 update_heartbeat() 更新节点心跳时间 ✅ 已实现 update_node_status() 更新节点状态 ✅ 已实现 check_and_update_health() 检查并标记不健康节点 ✅ 已实现 select_best_node() 选择负载最低的节点 ✅ 已实现 select_master_node() 选择主节点（一致性哈希） ✅ 已实现 is_master() 检查当前节点是否为主节点 ✅ 已实现 update_node_load() 更新节点负载信息 ✅ 已实现 with_lock() 使用分布式锁执行操作 ✅ 已实现
 ⚠️ 当前问题
- 节点状态只存在本地内存 ：没有持久化到 Redis
- 没有自动心跳发送 ：需要手动调用 update_heartbeat()
- 没有后台健康检查任务 ：需要手动调用 check_and_update_health()
- 没有租约机制 ：服务异常关闭时无法自动下线
### 2️⃣ SyncManager（状态同步模块）
文件位置 : cluster/sync.rs
 数据结构
```
/// 插件状态记录
pub struct PluginStateRecord {
    pub plugin_id: String,
    pub version: String,
    pub status: PluginStatus,
    pub node_id: String,
    pub updated_at: DateTime<Utc>,
}

/// 同步消息类型
pub enum SyncMessageType {
    StateUpdate,      // 状态更新
    StateDelete,      // 状态删除
    FullSyncRequest,  // 全量同步请求
    FullSyncResponse, // 全量同步响应
}

/// 同步消息
pub struct SyncMessage {
    pub msg_type: SyncMessageType,
    pub record: Option<PluginStateRecord>,
    pub source_node_id: String,
    pub timestamp: DateTime<Utc>,
}
``` 主要功能
方法 功能 状态 update_local_state() 更新本地状态并同步到远程 ✅ 已实现 get_local_state() 获取本地状态 ✅ 已实现 sync_to_remote() 通过 Redis Pub/Sub 发布状态更新 ✅ 已实现 sync_from_remote() 请求全量同步 ✅ 已实现 handle_sync_message() 处理接收到的同步消息 ✅ 已实现 remove_state() 删除状态并同步 ✅ 已实现
 ⚠️ 当前问题
- 没有订阅监听 ：虽然设计了 Pub/Sub，但没有启动后台订阅任务
- 没有在 PluginManager 中集成 ：安装/激活/卸载插件时没有调用同步
### 3️⃣ DeploymentCoordinator（部署协调模块）
文件位置 : cluster/deployment.rs
 数据结构
```
/// 部署策略
pub enum DeploymentStrategy {
    AllNodes,              // 所有节点
    SpecificNodes(Vec<String>), // 指定节点
    PrimaryOnly,           // 仅主节点
    RandomNodes(usize),    // 随机N个节点
}

/// 部署状态
pub enum DeploymentStatus {
    Pending,      // 待部署
    Deploying,    // 部署中
    Completed,    // 已完成
    Failed,       // 失败
    RollingBack,  // 回滚中
}

/// 部署任务
pub struct DeploymentTask {
    pub id: String,
    pub plugin_id: String,
    pub version: String,
    pub strategy: DeploymentStrategy,
    pub status: DeploymentStatus,
    pub target_nodes: Vec<String>,
    pub completed_nodes: Vec<String>,
    pub failed_nodes: Vec<String>,
}
``` 主要功能
方法 功能 状态 create_deployment_task() 创建部署任务 ✅ 已实现 resolve_target_nodes() 根据策略解析目标节点 ✅ 已实现
 ⚠️ 当前问题
- 只有创建任务，没有执行逻辑 ：缺少实际部署执行、状态追踪、回滚等功能
- 没有任务队列 ：无法管理多个部署任务
- 没有失败重试机制
## 📊 总结对比
模块 设计目标 实现程度 缺失功能 NodeManager 管理集群节点 70% Redis持久化、自动心跳、租约机制 SyncManager 跨节点状态同步 60% 订阅监听、PluginManager集成 DeploymentCoordinator 多节点部署协调 30% 任务执行、状态追踪、回滚机制

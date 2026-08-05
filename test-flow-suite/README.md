# 完整流程测试套件（test_expense）

一套完整的 cmx-flow 流程测试：多环节流程 + 基于 workspace node 的三区表单 + 待办中心六类全覆盖。

## 组成
- **definition.bpmn** — 差旅报销审批多环节流程（apply→review→金额网关→director(大额,待认领)/finance→end，带 cc 抄送）。
  所有 userTask 绑 `cmx:formKey="expense.form"`。
- **workspace-node.json** — `flow-form-expense` 完整工作区节点：explorer(明细项导航·demo.product-explorer) +
  content(报销单·task-form) + property(明细详情·demo.product-content)。存门户文件库 data/node/nodes.json。
- **seed-instances.py** — 起多个实例推进到不同环节，填满待办中心六类。可重复运行（businessKey 带批次号）。
- 后端种子 `expense.form → kind=workspace → flow-form-expense` 已写进 seed_form_bindings（重启自动恢复）。

## 复跑（服务已起、流程已发布装载后）
```bash
python3 test-flow-suite/seed-instances.py
```

## 测试用户（localStorage cmx_user_id，无需真登录）
| 用户 | 待办中心可测 |
|---|---|
| zhang | 我的待办（apply 环节）、发起流程 |
| li | 我已办（review 办结）、我的待办（review 待批） |
| wang / zhao | 待我认领（director 大额候选池） |
| qian | 我的待办（finance）、我已办 |
| observer | 抄送我的（review 触发 cc） |
| admin | 我发起的（全部实例） |

## 首次装载（新库/重启后）
1. 起服务（引擎装载已发布定义）。
2. 若 test_expense 未发布：`POST /api/flow/definitions/draft`(definition.bpmn) → `/publish` → 重启。
3. 若 workspace node 缺失：`POST /api/workspace-nodes`(workspace-node.json)。
4. `python3 seed-instances.py` 填数据。

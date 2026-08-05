#!/usr/bin/env python3
# 完整流程测试数据填充：起多个 test_expense 实例，推进到不同环节，
# 填满待办中心六类（我的待办/待我认领/发起流程/我发起的/抄送我的/我已办）。
# 幂等性：每次运行都新起实例（businessKey 带序号）；可重复运行叠加数据。
import json, urllib.request, urllib.error, sys, time

BASE = "http://localhost:8080"

def api(path, body=None, method=None):
    data = json.dumps(body).encode() if body is not None else None
    m = method or ("POST" if body is not None else "GET")
    req = urllib.request.Request(BASE + path, data=data,
                                 headers={"Content-Type": "application/json", "Accept": "application/json"},
                                 method=m)
    try:
        r = urllib.request.urlopen(req)
        j = json.load(r)
        return j.get("data", j)
    except urllib.error.HTTPError as e:
        print(f"  ! {method or 'GET'} {path} → {e.code}: {e.read().decode()[:200]}")
        return None

def start(bk, applicant, amount):
    """起一个 test_expense 实例，applicant/amount 进 variables。返回 instanceId。"""
    d = api("/api/flow/instances", {
        "definitionKey": "test_expense",
        "businessKey": bk,
        "variables": {"applicant": applicant, "amount": amount, "reason": "差旅报销测试"},
    })
    if d and d.get("id"):
        print(f"  ✓ 起实例 {bk} (¥{amount}, 申请人 {applicant}) → {d['id'][:8]}")
        return d["id"]
    print(f"  ✗ 起实例失败 {bk}")
    return None

def tasks_of(user, kind="todo"):
    d = api(f"/api/flow/tasks/my?assignee={user}&kind={kind}&page=1&pageSize=50")
    return (d or {}).get("tasks", [])

def complete(task_id, instance_id, decision="approve", comment="同意"):
    d = api(f"/api/flow/tasks/{task_id}/complete",
            {"instanceId": instance_id, "decision": decision, "comment": comment})
    return d is not None

def find_task(user, instance_id):
    for t in tasks_of(user):
        if t.get("instanceId") == instance_id:
            return t
    return None

# 时间戳后缀让 businessKey 唯一（可重复运行）
ts = int(time.time()) % 100000
print(f"=== 填充测试数据 (批次 {ts}) ===")

# ── 场景 1：停在 apply（zhang 的「我的待办」） ──
print("\n[1] 停在 apply — zhang 我的待办")
for i, amt in enumerate([1200, 3400], 1):
    start(f"EXP-{ts}-apply{i}", "admin", amt)

# ── 场景 2：小额，推进到 finance（qian 待办 + observer 抄送 + li 已办） ──
print("\n[2] 小额→finance — qian 待办 / observer 抄送 / li 已办")
iid = start(f"EXP-{ts}-small", "admin", 800)
if iid:
    t = find_task("zhang", iid)
    if t: complete(t["taskId"], iid, "approve", "申请提交")  # apply→review
    t = find_task("li", iid)
    if t: complete(t["taskId"], iid, "approve", "经理同意")  # review→gw→finance (小额直到 finance)

# ── 场景 3：大额，推进到 director（wang/zhao 的「待我认领」） ──
print("\n[3] 大额→director — wang/zhao 待认领")
iid = start(f"EXP-{ts}-big", "admin", 8800)
if iid:
    t = find_task("zhang", iid)
    if t: complete(t["taskId"], iid, "approve", "大额申请")   # apply→review
    t = find_task("li", iid)
    if t: complete(t["taskId"], iid, "approve", "经理同意大额")  # review→gw→director(候选池)

# ── 场景 4：跑到底（li/qian 已办 + 一条 COMPLETED 我发起的） ──
print("\n[4] 跑到底 — 完整闭环")
iid = start(f"EXP-{ts}-done", "admin", 600)
if iid:
    t = find_task("zhang", iid)
    if t: complete(t["taskId"], iid, "approve", "提交")
    t = find_task("li", iid)
    if t: complete(t["taskId"], iid, "approve", "经理批")
    t = find_task("qian", iid)
    if t: complete(t["taskId"], iid, "approve", "财务复核通过")  # finance→end (COMPLETED)

# ── 汇总各用户/各类计数 ──
print("\n=== 待办中心各类计数 ===")
def count(path, label):
    d = api(path)
    n = (d or {}).get("total", len((d or {}).get("tasks", [])))
    print(f"  {label}: {n}")
    return n

count("/api/flow/tasks/my?assignee=zhang&kind=todo&page=1&pageSize=50", "zhang 我的待办")
count("/api/flow/tasks/my?assignee=qian&kind=todo&page=1&pageSize=50", "qian 我的待办")
count("/api/flow/tasks/my?assignee=wang&kind=claimable&page=1&pageSize=50", "wang 待我认领")
count("/api/flow/tasks/my?assignee=zhao&kind=claimable&page=1&pageSize=50", "zhao 待我认领")
count("/api/flow/todos/initiated?user=admin&page=1&pageSize=50", "admin 我发起的")
count("/api/flow/todos/cc?user=observer&page=1&pageSize=50", "observer 抄送我的")
count("/api/flow/todos/done?user=li&page=1&pageSize=50", "li 我已办")
count("/api/flow/todos/done?user=qian&page=1&pageSize=50", "qian 我已办")
print("\n完成。")

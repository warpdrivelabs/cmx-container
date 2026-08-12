### 4.2 过滤操作符

查询参数写法：
```json
{
    "filter": {
        "name": {"$contains": "用户管理域"}
    }
}
```




#### 字符串OpValString运算符
| 操作符 | 含义 | 示例 |
|--------|------|------|
| `$eq` | 与一个值精确匹配 | `{ "name": { "$eq": "Jon Doe" } }` 等同于 `{ "name": "Jon Doe" }` |
| `$in` | 与值列表中的任意一项完全匹配（逻辑 OR） | `{ "name": { "$in": ["Alice", "Jon Doe"] } }` |
| `$not` | 排除精确匹配的值 | `{ "name": { "$not": "Jon Doe" } }` |
| `$notIn` | 排除列表中的任意一项 | `{ "name": { "$notIn": ["Jon Doe"] } }` |
| `$contains` | 字符串包含子串（区分大小写） | `{ "name": { "$contains": "Doe" } }` |
| `$containsAny` | 字符串包含列表中任意子串 | `{ "name": { "$containsAny": ["Doe", "Ali"] } }` |
| `$containsAll` | 字符串包含列表中所有子串 | `{ "name": { "$containsAll": ["Hello", "World"] } }` |
| `$notContains` | 字符串不包含子串 | `{ "name": { "$notContains": "Doe" } }` |
| `$notContainsAny` | 字符串不包含列表中任意子串 | `{ "name": { "$notContainsAny": ["Doe", "Ali"] } }` |
| `$startsWith` | 字符串以指定前缀开头（区分大小写） | `{ "name": { "$startsWith": "Jon" } }` |
| `$startsWithAny` | 字符串以列表中任意前缀开头 | `{ "name": { "$startsWithAny": ["Jon", "Al"] } }` |
| `$notStartsWith` | 字符串不以指定前缀开头 | `{ "name": { "$notStartsWith": "Jon" } }` |
| `$notStartsWithAny` | 字符串不以列表中任意前缀开头 | `{ "name": { "$notStartsWithAny": ["Jon", "Al"] } }` |
| `$endsWith` | 字符串以指定后缀结尾（区分大小写） | `{ "name": { "$endsWith": "Doe" } }` |
| `$endsWithAny` | 字符串以列表中任意后缀结尾 | `{ "name": { "$endsWithAny": ["Doe", "ice"] } }` |
| `$notEndsWith` | 字符串不以指定后缀结尾 | `{ "name": { "$notEndsWith": "Doe" } }` |
| `$notEndsWithAny` | 字符串不以列表中任意后缀结尾 | `{ "name": { "$notEndsWithAny": ["Doe", "ice"] } }` |
| `$lt` | 字典序小于 | `{ "name": { "$lt": "C" } }` |
| `$lte` | 字典序小于或等于 | `{ "name": { "$lte": "C" } }` |
| `$gt` | 字典序大于 | `{ "name": { "$gt": "J" } }` |
| `$gte` | 字典序大于或等于 | `{ "name": { "$gte": "J" } }` |
| `$null` | 值为 `null` | `{ "name": { "$null": true } }` |
| `$containsCi` | 字符串包含子串（不区分大小写） | `{ "name": { "$containsCi": "doe" } }` |
| `$notContainsCi` | 字符串不包含子串（不区分大小写） | `{ "name": { "$notContainsCi": "doe" } }` |
| `$startsWithCi` | 字符串以指定前缀开头（不区分大小写） | `{ "name": { "$startsWithCi": "jon" } }` |
| `$notStartsWithCi` | 字符串不以指定前缀开头（不区分大小写） | `{ "name": { "$notStartsWithCi": "jon" } }` |
| `$endsWithCi` | 字符串以指定后缀结尾（不区分大小写） | `{ "name": { "$endsWithCi": "doe" } }` |
| `$notEndsWithCi` | 字符串不以指定后缀结尾（不区分大小写） | `{ "name": { "$notEndsWithCi": "doe" } }` |
| `$ilike` | 类似 SQL `ILIKE`，不区分大小写模糊匹配（需启用 `with-ilike` feature） | `{ "name": { "$ilike": "DoE" } }` |

💡 注意：$ilike 通常需要在 Cargo.toml 中启用对应特性，例如：
```toml
[dependencies]
your-orm = { version = "...", features = ["with-ilike"] }
```

#### 数字操作符(OpValInt32, OpValInt64, OpValFloat64）

| 操作符 | 含义 | 示例 |
|--------|------|------|
| `$eq` | 与一个值精确匹配 | `{ "age": { "$eq": 24 } }` 等同于 `{ "age": 24 }` |
| `$in` | 与值列表中的任意一项完全匹配 | `{ "age": { "$in": [23, 24] } }` |
| `$not` | 排除精确匹配的值 | `{ "age": { "$not": 24 } }` |
| `$notIn` | 排除列表中的任意一项 | `{ "age": { "$notIn": [24] } }` |
| `$lt` | 小于 | `{ "age": { "$lt": 30 } }` |
| `$lte` | 小于或等于 | `{ "age": { "$lte": 30 } }` |
| `$gt` | 大于 | `{ "age": { "$gt": 30 } }` |
| `$gte` | 大于或等于 | `{ "age": { "$gte": 30 } }` |
| `$null` | 值为 `null` | `{ "name": { "$null": true } }` |

#### 布尔操作符（OpValBool）

| 操作符 | 含义 | 示例 |
|--------|------|------|
| `$eq` | 与一个值精确匹配 | `{ "dev": { "$eq": true } }` 等同于 `{ "dev": true }` |
| `$not` | 排除精确匹配的值 | `{ "dev": { "$not": false } }` |
| `$null` | 值为 `null` | `{ "name": { "$null": true } }` |


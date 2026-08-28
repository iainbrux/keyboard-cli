# 性能

源码位置：`src/modules/performance/*`

模块作用：读取和写入鼠标性能相关设置。调用入口为 `client.performance`。

## 读取指定键位的线性轴设置

client.performance.getLinearAxisSetting(key)

**简要描述:**
读取指定键位的线性轴设置。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `key` | `number` | 是 | 要查询的键位序号。 |

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureLinearAxisParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `key` | `number` | 当前查询的键位序号。 |
| `triggerMode` | `number` | 触发方式，`0` 普通触发方式，`1` RT 触发方式。 |
| `pressTravel` | `number` | 触发按下行程。 |
| `rtPressTravel` | `number` | RT 触发按下行程。 |
| `rtReleaseTravel` | `number` | RT 触发释放行程。 |
| `topDeadZone` | `number` | 顶部死区。 |
| `bottomDeadZone` | `number` | 底部死区。 |
| `maxTravel` | `number` | 最大行程。 |

---

### 使用示例

**调用示例:**

```ts
const linearAxis = await client.performance.getLinearAxisSetting(1);
console.log(linearAxis.triggerMode, linearAxis.pressTravel);
```

## 设置指定键位的线性轴参数

client.performance.setLinearAxisSetting(param)

**简要描述:**
设置指定键位的线性轴参数。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `param.key` | `number` | 是 | 要设置的键位序号。 |
| `param.triggerMode` | `number` | 是 | 触发方式，`0` 普通触发方式，`1` RT 触发方式。 |
| `param.pressTravel` | `number` | 是 | 触发按下行程。 |
| `param.rtPressTravel` | `number` | 是 | RT 触发按下行程。 |
| `param.rtReleaseTravel` | `number` | 是 | RT 触发释放行程。 |
| `param.topDeadZone` | `number` | 是 | 顶部死区。 |
| `param.bottomDeadZone` | `number` | 是 | 底部死区。 |
| `param.maxTravel` | `number` | 是 | 最大行程。 |

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureLinearAxisSettingResultParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `row` | `number` | 设备返回的键位行号。 |
| `col` | `number` | 设备返回的键位列号。 |
| `pressTravel` | `number` | 触发按下行程。 |
| `rtPressTravel` | `number` | RT 触发按下行程。 |
| `rtReleaseTravel` | `number` | RT 触发释放行程。 |
| `topDeadZone` | `number` | 顶部死区。 |
| `bottomDeadZone` | `number` | 底部死区。 |
| `maxTravel` | `number` | 最大行程。 |

---

### 使用示例

**调用示例:**

```ts
const result = await client.performance.setLinearAxisSetting({
  key: 1,
  triggerMode: 1,
  pressTravel: 100,
  rtPressTravel: 80,
  rtReleaseTravel: 60,
  topDeadZone: 5,
  bottomDeadZone: 5,
  maxTravel: 400,
});
console.log(result.row, result.col);
```

# 宏

模块作用：读取和写入宏模式、宏动作数据。调用入口为 `client.macro`。

## 读取指定宏的模式配置

client.macro.getMacroMode(param)

**简要描述:**
读取指定宏的模式配置。

---

### 参数

| 参数            | 类型      | 是否必需 | 说明           |
| --------------- | --------- | ---- | -------------- |
| `param.macroId` | `number`  | 是   | 宏 ID。        |
| `param.mode`    | `number`  | 否   | 读取时不需要。 |
| `param.actNum`  | `number`  | 否   | 读取时不需要。 |
| `param.repNum`  | `number`  | 否   | 读取时不需要。 |
| `param.valid`   | `boolean` | 否   | 读取时不需要。 |

---

### 返回值

* **总体类型:** `Promise<IMacroMode>`

**返回字段:**

| 字段      | 类型      | 说明         |
| --------- | --------- | ------------ |
| `macroId` | `number`  | 宏 ID。      |
| `valid`   | `boolean` | 宏是否有效。 |
| `actNum`  | `number`  | 动作数量。   |
| `repNum`  | `number`  | 重复次数。   |
| `mode`    | `number`  | 宏模式。     |

---

### 使用示例

**调用方式:**

```ts
const mode = await client.macro.getMacroMode({ macroId });
```

**调用示例:**

```ts
const mode = await client.macro.getMacroMode({
  macroId: 1,
});
console.log(mode.valid, mode.actNum, mode.repNum);
```

## 写入指定宏的模式配置

client.macro.setMacroMode(param)

**简要描述:**
写入指定宏的模式配置。

---

### 参数

| 参数            | 类型      | 是否必需 | 说明         |
| --------------- | --------- | ---- | ------------ |
| `param.macroId` | `number`  | 是   | 宏 ID。      |
| `param.valid`   | `boolean` | 是   | 宏是否有效。 |
| `param.actNum`  | `number`  | 是   | 动作数量。   |
| `param.repNum`  | `number`  | 是   | 重复次数。   |
| `param.mode`    | `number`  | 是   | 宏模式。     |

---

### 返回值

* **总体类型:** `Promise<IMacroMode>`
* **描述:** 返回设备确认后的宏模式配置。

---

### 使用示例

**调用方式:**

```ts
const result = await client.macro.setMacroMode(param);
```

**调用示例:**

```ts
await client.macro.setMacroMode({
  macroId: 1,
  valid: true,
  actNum: 2,
  repNum: 1,
  mode: 1,
});
```

## 读取指定宏从某个偏移开始的动作数据

client.macro.getMacroData(param)

**简要描述:**
读取指定宏从某个偏移开始的动作数据。

---

### 参数

| 参数            | 类型            | 是否必需 | 说明           |
| --------------- | --------------- | ---- | -------------- |
| `param.macroId` | `number`        | 是   | 宏 ID。        |
| `param.offset`  | `number`        | 是   | 读取偏移。     |
| `param.actions` | `MacroAction[]` | 否   | 读取时不需要。 |

---

### 返回值

* **总体类型:** `Promise<IMacroData>`

**返回字段:**

| 字段      | 类型            | 说明       |
| --------- | --------------- | ---------- |
| `macroId` | `number`        | 宏 ID。    |
| `offset`  | `number`        | 数据偏移。 |
| `actions` | `MacroAction[]` | 动作列表。 |

`MacroAction` 字段：

| 字段      | 类型     | 说明                         |
| --------- | -------- | ---------------------------- |
| `status`  | `number` | 动作状态。                   |
| `delay`   | `number` | 延迟，最大值为 `0x7fff`。    |
| `keyCode` | `number` | keycode，最大值为 `0xffff`。 |

---

### 使用示例

**调用方式:**

```ts
const data = await client.macro.getMacroData({ macroId, offset });
```

**调用示例:**

```ts
const data = await client.macro.getMacroData({
  macroId: 1,
  offset: 0,
});
console.log(data.actions);
```

## 写入指定宏从某个偏移开始的动作数据

client.macro.setMacroData(param)

**简要描述:**
写入指定宏从某个偏移开始的动作数据。

---

### 参数

| 参数            | 类型            | 是否必需 | 说明                                |
| --------------- | --------------- | ---- | ----------------------------------- |
| `param.macroId` | `number`        | 是   | 宏 ID。                             |
| `param.offset`  | `number`        | 是   | 写入偏移。                          |
| `param.actions` | `MacroAction[]` | 是   | 动作列表。每个动作会编码为 4 字节。 |

---

### 返回值

* **总体类型:** `Promise<IMacroData>`
* **描述:** 返回设备确认后的宏动作数据。

---

### 使用示例

**调用方式:**

```ts
const result = await client.macro.setMacroData(param);
```

**调用示例:**

```ts
await client.macro.setMacroData({
  macroId: 1,
  offset: 0,
  actions: [
    { status: 1, delay: 20, keyCode: 0x0004 },
    { status: 0, delay: 20, keyCode: 0x0004 },
  ],
});
```

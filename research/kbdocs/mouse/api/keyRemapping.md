# 按键映射

模块作用：读取和写入鼠标按键映射。调用入口为 `client.keyRemapping`。

## 读取当前按键映射

client.keyRemapping.queryButtons()

**简要描述:**
读取当前按键映射。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<KeyRemappingResult>`

**返回字段:**

| 字段             | 类型     | 说明                   |
| ---------------- | -------- | ---------------------- |
| `leftKey`        | `number` | 左键映射 keycode。     |
| `rightKey`       | `number` | 右键映射 keycode。     |
| `middleKey`      | `number` | 中键映射 keycode。     |
| `forwardKey`     | `number` | 前进键映射 keycode。   |
| `backKey`        | `number` | 后退键映射 keycode。   |
| `scrollForward`  | `number` | 滚轮前向映射 keycode。 |
| `scrollBackward` | `number` | 滚轮后向映射 keycode。 |
| `diyKey`         | `number` | 自定义键映射 keycode。 |

---

### 使用示例

**调用方式:**

```ts
const buttons = await client.keyRemapping.queryButtons();
```

**调用示例:**

```ts
const buttons = await client.keyRemapping.queryButtons();
console.log(buttons.leftKey, buttons.rightKey);
```

## 写入鼠标按键映射

client.keyRemapping.setButtons(param)

**简要描述:**
写入鼠标按键映射。

---

### 参数

| 参数                   | 类型     | 是否必需 | 说明                   |
| ---------------------- | -------- | ---- | ---------------------- |
| `param.leftKey`        | `number` | 是   | 左键映射 keycode。     |
| `param.rightKey`       | `number` | 是   | 右键映射 keycode。     |
| `param.middleKey`      | `number` | 是   | 中键映射 keycode。     |
| `param.forwardKey`     | `number` | 是   | 前进键映射 keycode。   |
| `param.backKey`        | `number` | 是   | 后退键映射 keycode。   |
| `param.scrollForward`  | `number` | 是   | 滚轮前向映射 keycode。 |
| `param.scrollBackward` | `number` | 是   | 滚轮后向映射 keycode。 |
| `param.diyKey`         | `number` | 是   | 自定义键映射 keycode。 |

---

### 返回值

* **总体类型:** `Promise<KeyRemappingResult>`
* **描述:** 返回设备确认后的按键映射。

---

### 使用示例

**调用方式:**

```ts
const result = await client.keyRemapping.setButtons(param);
```

**调用示例:**

```ts
const buttons = await client.keyRemapping.queryButtons();

const result = await client.keyRemapping.setButtons({
  ...buttons,
  middleKey: 0x0004,
});
```

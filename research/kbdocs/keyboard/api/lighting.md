# 键盘灯光

## 关闭键盘灯光

ServiceKeyboard.closedLighting()

**简要描述:**
关闭键盘的所有灯光效果。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个布尔值。
* **解析值:** `true` 如果灯光成功关闭，否则可能为 `false` 或 Promise 被拒绝 (具体行为需查阅实现细节)。

---

### 使用示例

```js
async function turnOffKeyboardLights() {
  try {
    const success = await ServiceKeyboard.closedLighting();
    if (success) {
      console.log('键盘灯光已关闭。');
    } else {
      console.log('关闭键盘灯光失败或指令未完全执行。');
    }
  } catch (error) {
    console.error('关闭键盘灯光时发生错误:', error);
  }
}

turnOffKeyboardLights();
```

## 获取键盘灯光配置

ServiceKeyboard.getLighting()

**简要描述:**
获取当前键盘主要的灯光配置信息。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<LightMode>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个 `LightMode` 对象，包含当前键盘灯光的详细配置。
* **解析对象结构 (`LightMode`):**

| 字段名称        | 类型              | 描述                                                                 | 示例值       |
|-----------------|-------------------|----------------------------------------------------------------------|--------------------|
| `open`          | `boolean`         | 灯光是否开启。                                                          | `true`             |
| `direction`     | `boolean`         | 灯效方向 (例如，动态效果的流动方向)。`true` 表示正向，`false` 反向。 | `true`             |
| `superResponse` | `boolean`         | 是否启用超强响应模式（通常指按键触发的快速反馈灯效）。                     | `false`            |
| `speed`         | `number`          | 灯光效果的速度。                                                        | `3` (示例级别)     |
| `colors`        | `string[]`        | 灯光效果所使用的颜色数组 (例如 `["#FF0000", "#00FF00"]`)。             | `["#FFFFFF"]`      |
| `mode`          | `number`          | 当前灯光模式。`0` 表示关闭, `1-20` (或其他范围) 表示预设的动态效果, `21` (或其他特定值) 表示自定义模式。 | `1` (示例动态模式) |
| `luminance`     | `number`          | 灯光亮度。                                                              | `100` (示例级别)   |
| `sleepDelay`    | `number`          | 灯光进入休眠状态前的延迟时间 (单位可能为秒或分钟)。                          | `600` (示例值)     |
| `staticColor`   | `number`          | 在静态灯光模式下所使用的颜色模式或索引。                                    | `0`                |
| `type`          | `LightModeType`   | 灯光模式的分类或类型 (具体枚举值需查阅 `LightModeType` 定义)。           | (依赖 `LightModeType`) |

 更多关于 `LightMode` 模型的详细结构，请 [查看lightMode的模型](/keyboard/model#灯光)。

---

### 使用示例

```js
async function fetchKeyboardLightingConfig() {
  try {
    const lightingConfig = await ServiceKeyboard.getLighting();
    console.log('当前键盘灯光配置:', lightingConfig);
    // console.log('灯光模式:', lightingConfig.mode);
    // console.log('亮度:', lightingConfig.luminance);
  } catch (error) {
    console.error('获取键盘灯光配置失败:', error);
  }
}

fetchKeyboardLightingConfig();
```

## 设置键盘灯光配置

ServiceKeyboard.setLighting()

**简要描述:**
应用新的灯光配置到键盘。

---

### 参数

| 参数名称   | 类型       | 描述                                   | 是否必需 | 默认值 |
|------------|------------|----------------------------------------|----------|--------|
| `lightMode`| `LightMode`| 一个 `LightMode` 对象，包含要设置的灯光配置。 | 是       | 无     |

* 关于 `LightMode` 对象的详细结构，请 [查看lightMode的模型](/keyboard/model#灯光)。

---

### 返回值

**总体类型:** `同入参`

---

### 使用示例

```js
async function applyKeyboardLightingConfig(newConfig: LightMode) {
  try {
    // newConfig 示例 (具体值需符合 LightMode 定义):
    // const exampleConfig: LightMode = {
    //   open: true,
    //   mode: 5, // 假设是某种动态效果
    //   speed: 4,
    //   luminance: 80,
    //   colors: ["#00FF00"],
    //   direction: true,
    //   superResponse: false,
    //   sleepDelay: 300,
    //   staticColor: 0,
    //   type: LightModeType.Effect, // 假设的 LightModeType 枚举值
    // };
    await ServiceKeyboard.setLighting(newConfig);
    console.log('键盘灯光配置已更新。');
  } catch (error) {
    console.error('设置键盘灯光配置失败:', error);
  }
}

// 假设 myCustomLightMode 是一个有效的 LightMode 对象
// applyKeyboardLightingConfig(myCustomLightMode);
```

## 获取键盘装饰灯灯光配置

ServiceKeyboard.getLogoLighting()

**简要描述:**
获取键盘上装饰灯的当前灯光配置信息。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<LightMode>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个 `LightMode` 对象，包含当前装饰灯的详细配置。其结构与主键盘灯光配置 (`ServiceKeyboard.getLighting()` 返回的) 类似，但可能具有不同的模式范围或特定于装饰灯的属性。
* **解析对象结构 (`LightMode`):**

| 字段名称        | 类型              | 描述                                                                 | 示例值      |
|-----------------|-------------------|----------------------------------------------------------------------|--------------------|
| `open`          | `boolean`         | 装饰灯灯光是否开启。                                                      | `true`             |
| `direction`     | `boolean`         | 灯效方向。`true` 正向, `false` 反向。                                  | `true`             |
| `superResponse` | `boolean`         | 是否启用超强响应模式。                                                  | `false`            |
| `speed`         | `number`          | 灯光效果的速度。                                                        | `3`                |
| `colors`        | `string[]`        | 灯光效果所使用的颜色数组。                                                | `["#0000FF"]`      |
| `mode`          | `number`          | 当前装饰灯灯光模式 (例如，动态灯效范围可能为 `1-4`)。                       | `1`                |
| `luminance`     | `number`          | 装饰灯灯光亮度。                                                          | `100`              |
| `sleepDelay`    | `number`          | 灯光休眠时间。                                                          | `600`              |
| `staticColor`   | `number`          | 静态灯光颜色模式。                                                      | `0`                |
| `type`          | `LightModeType`   | 灯光模式的分类或类型。                                                  | (依赖 `LightModeType`) |

更多关于 `LightMode` 模型的详细结构，请 [查看lightMode的模型](/keyboard/model#灯光)。

---

### 使用示例

```js
async function fetchLogoLightingConfig() {
  try {
    const logoConfig = await ServiceKeyboard.getLogoLighting();
    console.log('当前装饰灯灯光配置:', logoConfig);
  } catch (error) {
    console.error('获取装饰灯灯光配置失败:', error);
  }
}

fetchLogoLightingConfig();
```

## 设置键盘装饰灯灯光配置

ServiceKeyboard.setLogoLighting()

**简要描述:**
应用新的灯光配置到键盘的装饰灯。

---

### 参数

| 参数名称   | 类型       | 描述                                        | 是否必需 | 默认值 |
|------------|------------|---------------------------------------------|----------|--------|
| `lightMode`| `LightMode`| 一个 `LightMode` 对象，包含要设置的装饰灯灯光配置。 | 是       | 无     |

* 关于 `LightMode` 对象的详细结构，请 [查看lightMode的模型](/keyboard/model#灯光)。

---

### 返回值

**总体类型:** `同入参`

---

### 使用示例

```js
async function applyLogoLightingConfig(newLogoConfig: LightMode) {
  try {
    await ServiceKeyboard.setLogoLighting(newLogoConfig);
    console.log('装饰灯灯光配置已更新。');
  } catch (error) {
    console.error('设置装饰灯灯光配置失败:', error);
  }
}

// 假设 myCustomLogoLightMode 是一个有效的 LightMode 对象
// applyLogoLightingConfig(myCustomLogoLightMode);
```

### 注意事项

::: tip

* `type`: 键盘的装饰灯只有static静态模式和dynamic动态模式两种模式，设置其他的值将会被重置为static静态模式。
:::

## 获取单键自定义灯光颜色

ServiceKeyboard.getCustomLighting()

**简要描述:**
获取键盘上单个按键的自定义 RGB 灯光颜色值。

---

### 参数

| 参数名称 | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|----------|----------|----------------------------------------------------------------------|----------|--------|
| `key`    | `number` | 要查询自定义颜色的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

---

### 返回值

* **总体类型:** `Promise<{ key: number; R: number; G: number; B: number }>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含指定按键的 `keyValue` 及其RGB颜色值的对象。
* **解析对象结构:**

| 字段名称 | 类型     | 描述                     | 示例值 (可能) |
|----------|----------|--------------------------|---------------|
| `key`    | `number` | 查询的按键的 `keyValue`。    | `80` (示例)   |
| `R`      | `number` | 红色 (Red) 分量值 (0-255)。 | `255`         |
| `G`      | `number` | 绿色 (Green) 分量值 (0-255)。| `0`           |
| `B`      | `number` | 蓝色 (Blue) 分量值 (0-255)。 | `0`           |

---

### 使用示例

```js
async function fetchCustomKeyColor(targetKeyValue: number) {
  try {
    const colorInfo = await ServiceKeyboard.getCustomLighting(targetKeyValue);
    console.log(`按键 ${colorInfo.key} 的自定义颜色: R=${colorInfo.R}, G=${colorInfo.G}, B=${colorInfo.B}`);
  } catch (error) {
    console.error(`获取按键 ${targetKeyValue} 自定义颜色失败:`, error);
  }
}

// 示例：获取 keyValue 为 80 的按键的颜色
// const exampleKey = 80;
// fetchCustomKeyColor(exampleKey);
```

## 设置单键自定义灯光颜色

ServiceKeyboard.setCustomLighting()

**简要描述:**
为键盘上的单个按键设置自定义 RGB 灯光颜色。这通常在键盘灯光模式设置为自定义模式后生效。

---

### 参数

| 参数名称   | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|------------|----------|----------------------------------------------------------------------|----------|--------|
| `data`     | `object` | 一个包含按键及其 RGB 颜色配置的对象。                                    | 是       | 无     |
| `data.key` | `number` | 要设置自定义颜色的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `data.r`   | `number` | 红色 (Red) 分量值 (0-255)。                                            | 是       | 无     |
| `data.g`   | `number` | 绿色 (Green) 分量值 (0-255)。                                          | 是       | 无     |
| `data.b`   | `number` | 蓝色 (Blue) 分量值 (0-255)。                                            | 是       | 无     |

---

### 返回值

* **总体类型:** `同入参`

---

### 使用示例

```js
async function setCustomKeyColor(config: { key: number; r: number; g: number; b: number }) {
  try {
    // config 示例:
    const exampleConfig: LightMode = {
      open: true,
      mode: 5, // 假设是某种动态效果
      speed: 4,
      luminance: 80,
      colors: ["#00FF00"],
      direction: true,
      superResponse: false,
      sleepDelay: 300,
      staticColor: 0,
      type: "custom", //需要先设定键盘灯光为自定义模式
    };
    await ServiceKeyboard.setLighting(newConfig);
    // const colorConfig = { key: 80, R: 255, G: 0, B: 255 }; // 将 keyValue 80 的键设置为紫色
    await ServiceKeyboard.setCustomLighting(config);
    console.log(`按键 ${config.key} 的自定义颜色已设置为 R=${config.R}, G=${config.G}, B=${config.B}`);
  } catch (error) {
    console.error('设置单键自定义颜色失败:', error);
  }
}

// 示例调用
// const exampleColorSetting = { key: 80, R: 0, G: 255, B: 0 }; // 设置为绿色
// setCustomKeyColor(exampleColorSetting);
```

### 注意事项

::: tip

* `data.key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
* 要使自定义颜色生效，键盘的主灯光模式 (`ServiceKeyboard.setLighting`) 需要设置type为特定的"custom"值。
:::
* 记得自定义完成以后在下发saveCustomLighting保存命令哦，不需要每一条都发。

---

## 保存自定义灯光颜色

ServiceKeyboard.saveCustomLighting();

**简要描述:**

当自定义灯光发送完成以后要发送一个自定义保存命令下去。

---

### 参数

无

### 返回值

null

---

### 使用示例

```js
async function saveCustomLighting() {
  try {
    // config 示例:
    const exampleConfig: LightMode = {
      open: true,
      mode: 5, // 假设是某种动态效果
      speed: 4,
      luminance: 80,
      colors: ["#00FF00"],
      direction: true,
      superResponse: false,
      sleepDelay: 300,
      staticColor: 0,
      type: "custom", //需要先设定键盘灯光为自定义模式
    };

   await ServiceKeyboard.setCustomLighting(config);
  // 最后一步发送保存
  await ServiceKeyboard.saveCustomLighting(newConfig);
  
  } catch (error) {
  }
}


```

### 注意事项

::: tip
可以在切换页面，或者切换点击事件的时候进行切换。
:::

---

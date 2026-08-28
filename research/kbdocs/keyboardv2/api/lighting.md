# 键盘灯光

## 获取灯光基础配置

ServiceKeyboard.getLightingBase()

**简要描述:**
获取指定区域的灯光基础配置信息。

---

### 参数

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `params` | `object` | 是 | 描述查询区域和配置类型的对象 |
| `params.area` | `string` | 是 | 灯光区域，例如`Keyboard`,`Decorate1` |
| `params.config` | `string` | 是 | 配置类型，例如 `Base`,`Palette`,`ColorCorrection` |
| `params.lamp` | `string` | 否 | 描述灯的类型，例如`SingleLighting`,`DoubleLighting` |

---

::: tip

* 特殊灯效区域api见下方文档
* 单灯位和双灯位见下方文档

:::

### 返回值

* **总体类型:** `Promise<DoubleLighting>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含灯光基础配置信息的对象。
* **解析对象结构 (`DoubleLighting`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `area` | `string` | 灯光区域 | `"Keyboard"` |
| `open` | `string` | 灯光开关状态 | `"Open"` |
| `mode` | `number` | 灯光模式 | `1` |
| `luminance` | `number` | 灯光亮度 | `80` |
| `speed` | `number` | 灯光速度 | `80` |
| `direction` | `string` | 灯光方向`Forward`或者`Backward` | `Forward` |
| `selectStaticColor` | `number` | 静态颜色选择 | `0` |

**返回值示例:**

```js
{
    "area": "Keyboard",
    "open": "Open",
    "mode": 1,
    "luminance": 80,
    "speed": 80,
    "direction": "Forward",
    "selectStaticColor": 0
}
```

---

### 使用示例

```typescript
async function getLightingBase() {
  try {
    const result = await ServiceKeyboard.getLightingBase({
      area: "Keyboard",
      config: "Base",
      lamp: "DoubleLighting"
    });
    console.log('灯光基础配置:', result);
  } catch (error) {
    console.error('获取灯光基础配置失败:', error);
  }
}

getLightingBase();
```

---

## 设置灯光基础配置

ServiceKeyboard.setLightingBase()

**简要描述:**
设置指定区域的灯光基础配置信息。

---

### 参数

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `params` | `object` | 是 | 描述设置区域和配置的对象 |
| `params.area` | `string` | 是 | 灯光区域，例如`Keyboard`,`Decorate1`|
| `params.config` | `string` | 是 | 配置类型，例如 `Base`,`Palette`,`ColorCorrection` |
| `params.data` | `object` | 是 | 要设置的灯光配置数据 |
| `params.data.open` | `string` | 是 | 灯光开关状态，例如 `"Open"` |
| `params.data.mode` | `number` | 是 | 灯光模式 |
| `params.data.luminance` | `number` | 是 | 灯光亮度 |
| `params.data.speed` | `number` | 是 | 灯光速度 |
| `params.data.direction` | `string` | 是 | 灯光方向`Forward`或者`Backward` |
| `params.data.selectStaticColor` | `number` | 是 | 静态颜色选择 |
| `params.lamp` | `string` | 否 | 描述灯的类型，例如`SingleLighting`,`DoubleLighting` |

---

### 返回值

* **总体类型:** `Promise<DoubleLighting>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含设置后的灯光基础配置信息的对象。
* **解析对象结构 (`DoubleLighting`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `area` | `string` | 灯光区域 | `"Keyboard"` |
| `open` | `string` | 灯光开关状态 | `"Open"` |
| `mode` | `number` | 灯光模式 | `0` |
| `luminance` | `number` | 灯光亮度 | `80` |
| `speed` | `number` | 灯光速度 | `80` |
| `direction` | `string` | 灯光方向`Forward`或者`Backward` | `"Forward"` |
| `selectStaticColor` | `number` | 静态颜色选择 | `0` |

**返回值示例:**

```js
{
    "area": "Keyboard",
    "open": "Open",
    "mode": 0,
    "luminance": 80,
    "speed": 80,
    "direction": "Forward",
    "selectStaticColor": 0
}
```

---

### 使用示例

```typescript
async function setLightingBase() {
  try {
    const result = await ServiceKeyboard.setLightingBase({
      area: "Keyboard",
      config: "Base",
      lamp: "SingleLighting",
      data: {
        open: "Open",
        mode: 0,
        luminance: 80,
        speed: 80,
        direction: "Forward",
        selectStaticColor: 0
      }
    });
    console.log('设置灯光基础配置结果:', result);
  } catch (error) {
    console.error('设置灯光基础配置失败:', error);
  }
}

setLightingBase();
```

---

## 获取灯光调色板配置

ServiceKeyboard.getLightingPalette()

**简要描述:**
获取指定区域的灯光调色板配置信息。

---

### 参数

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `params` | `object` | 是 | 描述查询区域和配置类型的对象 |
| `params.area` | `string` | 是 | 灯光区域，例如`Keyboard`,`Decorate1` |
| `params.config` | `string` | 是 | 配置类型，例如 `Base`,`Palette`,`ColorCorrection` |

---

### 返回值

* **总体类型:** `Promise<ILightingPalette>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含灯光调色板配置信息的对象。
* **解析对象结构 (`ILightingPalette`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `staticColors` | `Array<IColorInfo>` | 静态颜色数组 | - |

**颜色信息结构 (`IColorInfo`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `R` | `number` | 红色分量值 (0-255) | `255` |
| `G` | `number` | 绿色分量值 (0-255) | `0` |
| `B` | `number` | 蓝色分量值 (0-255) | `0` |
| `H` | `number` | 亮度值 (0-255) | `255` |

**返回值示例:**

```js
{
    "staticColors": [
        {
            "R": 0,
            "G": 0,
            "B": 0,
            "H": 255
        },
        {
            "R": 255,
            "G": 0,
            "B": 0,
            "H": 255
        },
        {
            "R": 0,
            "G": 255,
            "B": 0,
            "H": 255
        },
        {
            "R": 255,
            "G": 255,
            "B": 0,
            "H": 255
        },
        {
            "R": 0,
            "G": 0,
            "B": 255,
            "H": 255
        },
        {
            "R": 255,
            "G": 0,
            "B": 255,
            "H": 255
        },
        {
            "R": 0,
            "G": 255,
            "B": 255,
            "H": 255
        },
        {
            "R": 255,
            "G": 255,
            "B": 255,
            "H": 255
        }
    ]
}
```

---

### 使用示例

```typescript
async function getLightingPalette() {
  try {
    const result = await ServiceKeyboard.getLightingPalette({
      area: "Keyboard",
      config: "Palette"
    });
    console.log('灯光调色板配置:', result);
  } catch (error) {
    console.error('获取灯光调色板配置失败:', error);
  }
}

getLightingPalette();
```

---

## 设置灯光调色板配置

ServiceKeyboard.setLightingPalette()

**简要描述:**
设置指定区域的灯光调色板配置信息。

---

### 参数

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `params` | `object` | 是 | 描述设置区域和配置的对象 |
| `params.area` | `string` | 是 | 灯光区域，例如`Keyboard`,`Decorate1` |
| `params.config` | `string` | 是 | 配置类型，例如 `Base`,`Palette`,`ColorCorrection` |
| `params.data` | `object` | 是 | 要设置的调色板配置数据 |
| `params.data.staticColors` | `Array<IColorInput>` | 是 | 静态颜色数组 |

**颜色输入结构 (`IColorInput`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `R` | `number` | 红色分量值 (0-255) | `255` |
| `G` | `number` | 绿色分量值 (0-255) | `0` |
| `B` | `number` | 蓝色分量值 (0-255) | `0` |

---

### 返回值

* **总体类型:** `Promise<ILightingPalette>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含设置后的灯光调色板配置信息的对象。
* **解析对象结构 (`ILightingPalette`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `staticColors` | `Array<IColorInfo>` | 静态颜色数组 | - |

**颜色信息结构 (`IColorInfo`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `R` | `number` | 红色分量值 (0-255) | `255` |
| `G` | `number` | 绿色分量值 (0-255) | `0` |
| `B` | `number` | 蓝色分量值 (0-255) | `0` |
| `H` | `number` | 亮度值 (0-255) | `0` |

**返回值示例:**

```js
{
    "staticColors": [
        {
            "R": 0,
            "G": 0,
            "B": 0,
            "H": 0
        },
        {
            "R": 255,
            "G": 0,
            "B": 0,
            "H": 0
        },
        {
            "R": 0,
            "G": 219,
            "B": 255,
            "H": 0
        },
        {
            "R": 255,
            "G": 255,
            "B": 0,
            "H": 0
        },
        {
            "R": 0,
            "G": 0,
            "B": 255,
            "H": 0
        },
        {
            "R": 255,
            "G": 0,
            "B": 255,
            "H": 0
        },
        {
            "R": 0,
            "G": 255,
            "B": 255,
            "H": 0
        },
        {
            "R": 255,
            "G": 255,
            "B": 255,
            "H": 0
        }
    ]
}
```

---

### 使用示例

```typescript
async function setLightingPalette() {
  try {
    const result = await ServiceKeyboard.setLightingPalette({
      area: "Keyboard",
      config: "Palette",
      data: {
        staticColors: [
          {
            R: 0,
            G: 0,
            B: 0
          },
          {
            R: 255,
            G: 0,
            B: 0
          },
          {
            R: 0,
            G: 219,
            B: 255
          },
          {
            R: 255,
            G: 255,
            B: 0
          },
          {
            R: 0,
            G: 0,
            B: 255
          },
          {
            R: 255,
            G: 0,
            B: 255
          },
          {
            R: 0,
            G: 255,
            B: 255
          },
          {
            R: 255,
            G: 255,
            B: 255
          }
        ]
      }
    });
    console.log('设置灯光调色板配置结果:', result);
  } catch (error) {
    console.error('设置灯光调色板配置失败:', error);
  }
}

setLightingPalette();
```

## 获取自定义灯光配置

ServiceKeyboard.getLightingCustom()

**简要描述:**
获取键盘的自定义灯光配置信息，返回一个二维数组，表示键盘上每个按键的RGB颜色值和自定义状态。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<Array<Array<ICustomLightingInfo>>>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个二维数组，每个元素代表键盘上对应位置的按键灯光信息。
* **解析对象结构 (`ICustomLightingInfo`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `R` | `number` | 红色分量值 (0-255) | `0` |
| `G` | `number` | 绿色分量值 (0-255) | `207` |
| `B` | `number` | 蓝色分量值 (0-255) | `244` |
| `isCustom` | `boolean` | 是否为自定义颜色 | `false` |

**返回值示例:**

```js
[
    [
        {
            "R": 0,
            "G": 207,
            "B": 244,
            "isCustom": false
        },
        // ... 更多按键
    ],
    // ... 更多行
]
```

---

### 使用示例

```typescript
async function getLightingCustom() {
  try {
    const result = await ServiceKeyboard.getLightingCustom();
    console.log('自定义灯光配置:', result);
    // 可以遍历结果来获取每个按键的颜色信息
    result.forEach((row, rowIndex) => {
      row.forEach((key, colIndex) => {
        console.log(`按键[${rowIndex}][${colIndex}]的颜色: R=${key.R}, G=${key.G}, B=${key.B}, 是否自定义=${key.isCustom}`);
      });
    });
  } catch (error) {
    console.error('获取自定义灯光配置失败:', error);
  }
}

getLightingCustom();
```

## 设置自定义灯光配置

ServiceKeyboard.setLightingCustom()

**简要描述:**
设置键盘的自定义灯光配置，可以为每个按键设置独立的RGB颜色值和自定义状态。

---

### 参数

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `params` | `object` | 是 | 描述设置区域和配置的对象 |
| `params.area` | `string` | 是 | 灯光区域，例如`Keyboard` |
| `params.protocol` | `string` | 是 | 协议类型，固定为`Custom` |
| `params.data` | `Array<Array<ICustomLightingInfo>>` | 是 | 二维数组，表示键盘上每个按键的灯光配置 |

**按键灯光信息结构 (`ICustomLightingInfo`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `R` | `number` | 红色分量值 (0-255) | `0` |
| `G` | `number` | 绿色分量值 (0-255) | `8` |
| `B` | `number` | 蓝色分量值 (0-255) | `10` |
| `isCustom` | `boolean` | 是否为自定义颜色 | `false` |

**参数示例:**

```js
{
    "area": "Keyboard",
    "protocol": "Custom",
    "data": [
        [
            {
                "R": 0,
                "G": 8,
                "B": 10,
                "isCustom": false
            },
            // ... 更多按键
        ],
        // ... 更多行
    ]
}
```

---

### 返回值

此方法没有返回值。

---

### 使用示例

```typescript
async function setLightingCustom() {
  try {
    const customLightingConfig = {
      area: "Keyboard",
      protocol: "Custom",
      data: [
        [
          {
            R: 0,
            G: 8,
            B: 10,
            isCustom: false
          },
          // ... 更多按键配置
        ],
        // ... 更多行配置
      ]
    };
    
    await ServiceKeyboard.setLightingCustom(customLightingConfig);
    console.log('自定义灯光配置已设置');
  } catch (error) {
    console.error('设置自定义灯光配置失败:', error);
  }
}

setLightingCustom();
```

## 获取装饰灯1自定义灯光配置

ServiceKeyboard.getDecorate1Custom()

**简要描述:**
获取装饰灯1的自定义灯光配置信息，返回一个二维数组，表示装饰灯上每个LED的RGB颜色值和自定义状态。

---

### 参数

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `params` | `object` | 是 | 描述装饰灯尺寸的对象 |
| `params.rows` | `number` | 是 | 装饰灯的行数 |
| `params.cols` | `number` | 是 | 装饰灯的列数 |
| `params.area` | `string` | 是 | 区域 | "Keyboard" | "Decorate1" | "Decorate2" | "Decorate3" | "Decorate4" | "Decorate5"

---

### 返回值

* **总体类型:** `Promise<Array<Array<ICustomLightingInfo>>>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个二维数组，每个元素代表装饰灯上对应位置的LED灯光信息。
* **解析对象结构 (`ICustomLightingInfo`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `R` | `number` | 红色分量值 (0-255) | `128` |
| `G` | `number` | 绿色分量值 (0-255) | `41` |
| `B` | `number` | 蓝色分量值 (0-255) | `49` |
| `isCustom` | `boolean` | 是否为自定义颜色 | `true` |

**返回值示例:**

```js
[
    [
        {
            "R": 128,
            "G": 41,
            "B": 49,
            "isCustom": true
        },
        // ... 更多LED
    ]
]
```

---

### 使用示例

```typescript
async function getDecorate1Custom() {
  try {
    const result = await ServiceKeyboard.getDecorate1Custom({
      rows: 1,
      cols: 22
      area:"Keyboard" | "Decorate1" | "Decorate2" | "Decorate3" | "Decorate4" | "Decorate5"
    });
    console.log('装饰灯1自定义灯光配置:', result);
    // 可以遍历结果来获取每个LED的颜色信息
    result.forEach((row, rowIndex) => {
      row.forEach((led, colIndex) => {
        console.log(`LED[${rowIndex}][${colIndex}]的颜色: R=${led.R}, G=${led.G}, B=${led.B}, 是否自定义=${led.isCustom}`);
      });
    });
  } catch (error) {
    console.error('获取装饰灯1自定义灯光配置失败:', error);
  }
}

getDecorate1Custom();
```

## 设置装饰灯1自定义灯光配置

ServiceKeyboard.setDecorate1Custom()

**简要描述:**
设置装饰灯1的自定义灯光配置，可以为每个LED设置独立的RGB颜色值和自定义状态。

---

### 参数

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `params` | `object` | 是 | 描述设置区域和配置的对象 |
| `params.area` | `string` | 是 | 灯光区域，固定为`Decorate1` |
| `params.protocol` | `string` | 是 | 协议类型，固定为`Custom` |
| `params.data` | `Array<Array<ICustomLightingInfo>>` | 是 | 二维数组，表示装饰灯上每个LED的灯光配置 |

**LED灯光信息结构 (`ICustomLightingInfo`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `R` | `number` | 红色分量值 (0-255) | `128` |
| `G` | `number` | 绿色分量值 (0-255) | `41` |
| `B` | `number` | 蓝色分量值 (0-255) | `49` |
| `isCustom` | `boolean` | 是否为自定义颜色 | `true` |

**参数示例:**

```js
{
    "area": "Decorate1",
    "protocol": "Custom",
    "data": [
        [
            {
                "R": 128,
                "G": 41,
                "B": 49,
                "isCustom": true
            },
            // ... 更多LED
        ]
    ]
}
```

---

### 返回值

此方法没有返回值。

---

### 使用示例

```typescript
async function setDecorate1Custom() {
  try {
    const customLightingConfig = {
      area: "Decorate1",
      protocol: "Custom",
      data: [
        [
          {
            R: 128,
            G: 41,
            B: 49,
            isCustom: true
          },
          // ... 更多LED配置
        ]
      ]
    };
    
    await ServiceKeyboard.setDecorate1Custom(customLightingConfig);
    console.log('装饰灯1自定义灯光配置已设置');
  } catch (error) {
    console.error('设置装饰灯1自定义灯光配置失败:', error);
  }
}

setDecorate1Custom();
```

## 获取灯光区域信息

ServiceKeyboard.getLightingArea()

**简要描述:**
获取键盘上所有灯光区域的信息，包括主键盘灯和装饰灯的数量、行列数等配置信息。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<ILightingAreaInfo>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含所有灯光区域信息的对象。
* **解析对象结构 (`ILightingAreaInfo`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `total` | `number` | 灯光区域 | `2` |
| `areas` | `Array<IAreaInfo>` | 灯光区域信息数组 | - |

**区域信息结构 (`IAreaInfo`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `index` | `number` | 灯效区域索引 | `0` |
| `count` | `number` | 灯效数量 | `20` |
| `rows` | `number` | 行数 | `6` |
| `cols` | `number` | 列数 | `15` |

**返回值示例:**

```js
{
    "total": 2,
    "areas": [
        {
            "index": 0,
            "count": 20,
            "rows": 6,
            "cols": 15
        },
        {
            "index": 1,
            "count": 5,
            "rows": 1,
            "cols": 22
        }
    ]
}
```

### 区域说明

* `total` 值说明：
  * `1`: 主键盘灯
  * `2`: 装饰灯1
  * `3`: 装饰灯2

---

### 使用示例

```typescript
async function getLightingArea() {
  try {
    const result = await ServiceKeyboard.getLightingArea();
    console.log('灯光区域信息:', result);
    
    // 遍历所有灯光区域
    result.areas.forEach(area => {
      console.log(`区域索引 ${area.index}:`);
      console.log(`- 灯效数量: ${area.count}`);
      console.log(`- 行数: ${area.rows}`);
      console.log(`- 列数: ${area.cols}`);
    });
  } catch (error) {
    console.error('获取灯光区域信息失败:', error);
  }
}

getLightingArea();
```

## 获取双灯效状态

ServiceKeyboard.getDoubleLighting()

**简要描述:**
获取键盘的双灯效状态配置信息。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<IDoubleLightingInfo>`
* **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含双灯效状态信息的对象。
* **解析对象结构 (`IDoubleLightingInfo`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `doubleLighting` | `number` | 双灯效状态 | `1` |

**返回值示例:**

```js
{
    "doubleLighting": 1
}
```

---

### 使用示例

```typescript
async function getDoubleLighting() {
  try {
    const result = await ServiceKeyboard.getDoubleLighting();
    console.log('双灯效状态:', result.doubleLighting);
  } catch (error) {
    console.error('获取双灯效状态失败:', error);
  }
}

getDoubleLighting();
```

:::tip

主要是用来控制单双灯位的命令

:::

## 获取特殊灯位（空格灯）

ServiceKeyboard.getSpecialLighting()

**简要描述:**
获取特殊灯位（空格键）灯珠数量，用于单独控制空格键位置的灯珠。

 ---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<{ specialLighting: number }>`
* **描述:** 返回空格键位置的灯珠数量。

**返回值示例:**

```js
{
    "specialLighting": 1
}
```

---

### 使用示例

```typescript
async function getSpecialLighting() {
  try {
    const result = await ServiceKeyboard.getSpecialLighting();
    console.log('空格键灯珠数量 specialLighting:', result.specialLighting);
  } catch (error) {
    console.error('获取空格键灯珠数量失败:', error);
  }
}

getSpecialLighting();
```

::::tip

主要用于读取空格键位置可控灯珠的数量。

::::

---

## 设置直驱灯光配置

ServiceKeyboard.setLightingDirectDrive()

**简要描述:**
设置指定区域的直驱灯光配置，通过直接驱动方式控制灯光效果。

---

### 参数

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `params` | `object` | 是 | 描述设置区域和直驱灯光配置的对象 |
| `params.area` | `string` | 是 | 灯光区域，可选值：`Keyboard`, `Decorate1`, `Decorate2`, `Decorate3`, `Decorate4`, `Decorate5` |
| `params.protocol` | `string` | 是 | 协议类型，固定为 `DirectDrive` |
| `params.data` | `Array<Array<ICustomLightingInfo>>` | 是 | 灯光配置二维数组，大小应与尺寸参数匹配 |
| `{ rows, cols }` | `object` | 否 | 灯光矩阵尺寸配置，默认值为 `{ rows: 6, cols: 21 }` |
| `rows` | `number` | 否 | 灯光矩阵行数 |
| `cols` | `number` | 否 | 灯光矩阵列数 |

**灯光信息结构 (`ICustomLightingInfo`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `R` | `number` | 红色分量值 (0-255) | `255` |
| `G` | `number` | 绿色分量值 (0-255) | `0` |
| `B` | `number` | 蓝色分量值 (0-255) | `0` |
| `isCustom` | `boolean` | 是否为自定义颜色 | `true` |

**参数示例:**

```js
// 默认尺寸 (6×21)
{
  "area": "Keyboard",
  "protocol": "DirectDrive",
  "data": [
    [
      { "R": 255, "G": 0, "B": 0, "isCustom": true },
      { "R": 0, "G": 255, "B": 0, "isCustom": true },
      // ... 共21个元素
    ],
    // ... 共6行
  ]
}

```

---

### 返回值

此方法没有返回值。

---

### 使用示例

```typescript
async function setLightingDirectDrive() {
  try {
    // 创建6×21的灯光配置数组（使用默认尺寸）
    const colorArray = Array(6).fill(0).map(() => 
      Array(21).fill(0).map(() => ({
        R: Math.floor(Math.random() * 256),
        G: Math.floor(Math.random() * 256),
        B: Math.floor(Math.random() * 256),
        isCustom: true
      }))
    );
    
    // 设置直驱灯光配置（使用默认尺寸）
    await ServiceKeyboard.setLightingDirectDrive({
      area: "Keyboard",
      protocol: "DirectDrive",
      data: colorArray
    });
    
    console.log('直驱灯光配置已设置（默认尺寸）');
  } catch (error) {
    console.error('设置直驱灯光配置失败:', error);
  }
}

// 使用自定义尺寸
async function setLightingDirectDriveWithCustomSize() {
  try {
    const rows = 3;
    const cols = 10;
    
    // 创建自定义尺寸的灯光配置数组
    const colorArray = Array(rows).fill(0).map(() => 
      Array(cols).fill(0).map(() => ({
        R: Math.floor(Math.random() * 256),
        G: Math.floor(Math.random() * 256),
        B: Math.floor(Math.random() * 256),
        isCustom: true
      }))
    );
    
    // 设置直驱灯光配置（使用自定义尺寸）
    await ServiceKeyboard.setLightingDirectDrive({
      area: "Keyboard",
      protocol: "DirectDrive",
      data: colorArray
    }, { rows, cols });
    
    console.log('直驱灯光配置已设置（自定义尺寸）');
  } catch (error) {
    console.error('设置直驱灯光配置失败:', error);
  }
}

setLightingDirectDrive();
setLightingDirectDriveWithCustomSize();
```

---

### 注意事项

::: tip

* 直驱灯光配置通过直接驱动方式控制灯光，可实现更精细的灯光效果。
* `area` 参数支持的取值为：`Keyboard`, `Decorate1`, `Decorate2`, `Decorate3`, `Decorate4`, `Decorate5`。
* `data` 参数必须是一个二维数组，其大小应与第二个参数中指定的 `rows` 和 `cols` 匹配。
* 第二个参数为可选参数，默认值为 `{ rows: 6, cols: 21 }`。
* 此接口适用于需要精确控制每个灯位颜色的场景。

:::

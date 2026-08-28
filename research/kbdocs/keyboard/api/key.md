# 布局/改建

## 初始化布局

ServiceKeyboard.defKey()

**简要描述:**
获取当前键盘的初始化布局模型，返回一个代表键盘完整布局的二维数组。

---

### 参数 

此方法不需要参数。

---

### 返回值 

*   **总体类型:** `Promise<IDefKeyInfo[][]>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个二维数组 (`IDefKeyInfo[][]`)。数组中的每个 `IDefKeyInfo` 对象代表一个按键的布局信息。
*   **解析对象结构 (`IDefKeyInfo`):**

| 字段名称 | 类型 | 描述 | 示例值 |
|---------|------|------|--------|
| `keyValue` | `number` | 按键的值 | `41` |
| `location` | `object` | 按键在键盘上的位置信息 | - |
| `location.row` | `number` | 按键所在的行号（从0开始） | `0` |
| `location.col` | `number` | 按键所在的列号（从0开始） | `0` |

**返回值示例:**
```js
[
    [   // 第0行
        {
            "keyValue": 41,
            "location": {
                "row": 1,
                "col": 0
            }
        },
        // ... 更多按键
    ],
    [   // 第1行
        {
            "keyValue": 43,
            "location": {
                "row": 2,
                "col": 0
            }
        },
        // ... 更多按键
    ],
    // ... 更多行
]
```

---

### 使用示例 

```typescript
async function fetchInitialLayout() {
  try {
    const layoutGrid = await ServiceKeyboard.defKey();
    console.log('初始化键盘布局:', layoutGrid);
    // layoutGrid 是一个二维数组, 例如:
    // [
    //   [IDefKeyInfo, IDefKeyInfo, ...], // 第一行按键
    //   [IDefKeyInfo, IDefKeyInfo, ...], // 第二行按键
    //   ...
    // ]
    if (layoutGrid.length > 0 && layoutGrid[0].length > 0) {
      // const firstKeyInfo = layoutGrid[0][0];
      // console.log('第一行第一个键的信息:', firstKeyInfo);
    }
  } catch (error) {
    console.error('获取初始化布局失败:', error);
  }
}

fetchInitialLayout();
```


## 获取布局

ServiceKeyboard.getLayoutKeyInfo()

**简要描述:**
根据指定的键（key）的 `keyValue` 和布局层（layout）获取该层级的详细布局信息。

---

### 参数 

| 参数名称   | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|------------|----------|----------------------------------------------------------------------|----------|--------|
| `object[]`   | `Array` | 一个包含请求参数的对象的数组。                                                 | 是       | 无     |
| `object.key` | `number` | 要查询的键的 `keyValue` 值。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `object.layout` | `number` | 要查询的布局层级。目前支持的值：<br/>`0`: Fn1层<br/>`1`: Fn2层<br/>`2`: Fn3层<br/>`3`: Fn4层 | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<IDefKeyInfo[][]>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个二维数组 (`IDefKeyInfo[][]`)，代表在指定布局层下，所有按键的布局信息。
*   **解析对象结构 (`IDefKeyInfo`):**
    *   关于 `IDefKeyInfo` 对象的详细结构，请 [查看IDefKeyInfo的模型](/keyboard/model#单键的布局模型)。

---

### 使用示例 

```typescript
async function fetchLayerLayout(targetKeyValue: number, layerIndex: number) {
  try {
    const layoutParams = [{ key: targetKeyValue, layout: layerIndex }];
    const layerLayoutGrid = await ServiceKeyboard.getLayoutKeyInfo(layoutParams);
    console.log(`键 ${targetKeyValue} 在布局层 ${layerIndex} 的完整布局信息:`, layerLayoutGrid);
    // layerLayoutGrid 结构与 defKey() 返回的类似，但反映的是指定 Fn 层的情况
  } catch (error) {
    console.error(`获取布局层 ${layerIndex} 信息失败:`, error);
  }
}

// 示例：假设我们要获取某个键 (例如 keyValue 为 80) 在 Fn1 层 (layout 0) 的布局信息
// const exampleKeyValue = 80;
// const exampleLayoutLayer = 0;
// fetchLayerLayout(exampleKeyValue, exampleLayoutLayer);
```

---

### 注意事项 

::: tip
*   `params.key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `params.layout`: 指的是Fn功能层。
:::


## 改键

ServiceKeyboard.setKey()

**简要描述:**
修改一个或多个特定按键在指定布局层上的键值。

---

### 参数 

| 参数名称        | 类型                                                        | 描述                                                                                                | 是否必需 | 默认值 |
|-----------------|-------------------------------------------------------------|-----------------------------------------------------------------------------------------------------|----------|--------|
| `keyConfigs`    | `Array<object>`                                             | 一个包含一个或多个按键配置对象的数组。每个对象代表一个要修改的按键。                                        | 是       | 无     |
| `keyConfigs[].key` | `number`                                                  | 要修改的键的 `keyValue` 值。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `keyConfigs[].layout`| `number`                                                | 要修改的键所在的布局层级。有效值：<br/>`0`: Fn1层<br/>`1`: Fn2层<br/>`2`: Fn3层<br/>`3`: Fn4层        | 是       | 无     |
| `keyConfigs[].value`| `number`                                                    | 要为当前对象指定的按键设置的新的键值。                                                                  | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<IDefKeyInfo[]>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为数组 (`IDefKeyInfo[]`)，代表改键操作后键盘的完整（或受影响层级的）最新布局信息。
*   **解析对象结构 (`IDefKeyInfo`):**
    *   关于 `IDefKeyInfo` 对象的详细结构，请 [查看IDefKeyInfo的模型](/keyboard/model#单键的布局模型)。
    *   **注意:** 单个改键操作的返回结果为 `[{ key: number; layout: number; value: number; }]`。当支持批量改键时，返回更新后的值是多个。

---

### 使用示例 

```typescript
async function remapMultipleKeys(configs: Array<{ key: number; layout: number; value: number }>) {
  try {
    // configs 示例:
    // const keyRemapConfigs = [
    //   { key: 80, layout: 0, value: 100 }, // 将 keyValue 80 在 Fn1层 改为 100
    //   { key: 81, layout: 0, value: 101 }  // 将 keyValue 81 在 Fn1层 改为 101
    // ];
    const updatedLayoutGrid = await ServiceKeyboard.setKey(configs);
    console.log('批量改键成功，新的布局信息:', updatedLayoutGrid);
    // updatedLayoutGrid 将是改键后的键盘布局
  } catch (error) {
    console.error('改键操作失败:', error);
  }
}

// 示例调用
// const exampleConfigs = [
//   { key: 80, layout: 0, value: 100 },
//   { key: 23, layout: 1, value: 55 }
// ];
// remapMultipleKeys(exampleConfigs);
```

---

### 注意事项 

::: tip
*   `keyConfigs[].key`: 此处 `key` 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `keyConfigs[].layout`: 指的是Fn功能层。
*   `keyConfigs[].value`: 指的是您希望分配给目标按键的新的键码或功能码。
:::


## 获取轴体

ServiceKeyboard.getAxis()

**简要描述:**
获取指定按键的轴体设置。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `keyValue` | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

### 返回值
**类型:** `Promise<{ axis: number }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含按键的轴体设置。

**返回值示例:**
```js
{
    "axis": 1  // 轴体ID
}
```

### 使用示例
```js
async function getKeyAxis(keyValue) {
  try {
    const result = await ServiceKeyboard.getAxis(keyValue);
    console.log(`按键 ${keyValue} 的轴体设置:`, result);
    return result;
  } catch (error) {
    console.error('获取轴体设置失败:', error);
    throw error;
  }
}

// 示例：获取 keyValue 为 80 的按键的轴体设置
// const exampleKey = 80;
// getKeyAxis(exampleKey);
```

### 注意事项

::: tip
*   `keyValue`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
:::



## 设置轴体

ServiceKeyboard.setAxis()

**简要描述:**
设置指定按键的轴体类型。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `keyValue` | `number` | 要设置的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `axis`   | `number` | 要设置的轴体ID。                                                                                    | 是       | 无     |

### 返回值
**类型:** `Promise<void>`

**描述:** 返回一个 `Promise`，该 `Promise` 在操作成功时解析为 `void`。

### 使用示例
```js
async function setKeyAxis(keyValue, axis) {
  try {
    await ServiceKeyboard.setAxis(keyValue, axis);
    console.log(`已设置按键 ${keyValue} 的轴体为 ${axis}`);
  } catch (error) {
    console.error('设置轴体失败:', error);
    throw error;
  }
}

// 示例：设置 keyValue 为 27 的按键的轴体为 1
// const exampleKey = 27;
// const exampleAxis = 1;
// setKeyAxis(exampleKey, exampleAxis);
```

### 注意事项
*   `keyValue`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `axis`: 轴体ID，用于指定要设置的轴体类型。



## 获取轴体列表

ServiceKeyboard.getAxisList()

**简要描述:**
获取键盘当前支持的轴体列表。

### 参数
此方法不需要参数。

### 返回值
**类型:** `Promise<{ hasAxisSetting: boolean; axisList: number[] }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含轴体设置状态和轴体列表。

**返回值示例:**
```js
{
    "hasAxisSetting": true,  // 是否有轴体设置
    "axisList": [           // 轴体列表
        7,                  // 轴体ID
        32,
        0,
        0,
        0,
        0,
        0,
        0
    ]
}
```

### 使用示例
```js
async function fetchAxisList() {
  try {
    const result = await ServiceKeyboard.getAxisList();
    console.log('轴体列表:', result);
    return result;
  } catch (error) {
    console.error('获取轴体列表失败:', error);
    throw error;
  }
}

// 示例：获取轴体列表
// fetchAxisList();
```

### 注意事项

::: tip
*   `hasAxisSetting`: 表示键盘是否支持轴体设置功能。
*   `axisList`: 数组中的每个数字代表一个轴体ID，0表示该位置没有轴体。
:::

## 全局功能轴体库查询

**简要描述:**
键盘支持轴体库

### 注意事项

::: tip
*   **轴体库的具体接口请联系我们获取。**
*   导入轴体库前，请确保数据格式正确。
:::



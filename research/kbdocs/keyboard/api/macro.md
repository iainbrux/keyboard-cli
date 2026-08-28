# 宏

## 获取宏设置

ServiceKeyboard.getMacro()

**简要描述:**
获取指定按键的宏设置。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `key`    | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

### 返回值
**类型:** `Promise<IMacroMode>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个 `IMacroMode` 对象，包含按键的宏设置。

**内容:** 关于 `IMacroMode` 对象的详细结构，请[查看IMacroMode的模型](/keyboard/model#宏)。

### 使用示例
```js
async function getMacroSettings(keyValue) {
  try {
    const result = await ServiceKeyboard.getMacro(keyValue);
    console.log(`按键 ${keyValue} 的宏设置:`, result);
    // 示例返回数据:
    // {
    //     "key": 17,
    //     "id": 2,
    //     "len": 0,
    //     "mode": 0,
    //     "num": 2,
    //     "delay": 3
    // }
    return result;
  } catch (error) {
    console.error('获取宏设置失败:', error);
    throw error;
  }
}

// 示例：获取 keyValue 为 80 的按键的宏设置
// const exampleKey = 80;
// getMacroSettings(exampleKey);
```

### 注意事项

::: tip
*   `key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
:::

## 设置宏

ServiceKeyboard.setMacro()

**简要描述:**
为指定按键设置宏功能。宏功能允许用户将一系列按键操作绑定到一个按键上，实现一键触发多个操作。

### 参数
| 参数名称     | 类型                    | 描述                                                                                                | 是否必需 | 默认值 |
| :----------- | :---------------------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `param`      | `IMacroMode`           | 宏模式设置。                                                                                        | 是       | 无     |
| `macros`     | `MacroType[]`          | 宏序列数组。                                                                                        | 是       | 无     |

### 参数详细说明

**IMacroMode 结构:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `index`  | `number` | 宏索引。         | `2`    |
| `len`    | `number` | 宏序列长度。     | `4`    |
| `mode`   | `number` | 宏执行模式：<br/>`0`: 单击执行<br/>`1`: 点击重复执行（再次点击停止）<br/>`2`: 按下重复执行（弹起立刻停止）<br/>`3`: 按下重复执行（弹起后完成此次宏后停止） | `0`    |
| `key`    | `number` | 按键的keyValue。 | `16`   |
| `num`    | `number` | 重复次数。       | `3`    |
| `delay`  | `number` | 延迟时间。       | `4`    |

**MacroType 结构:**
| 字段名称        | 类型     | 描述             | 示例值 |
| :-------------- | :------- | :--------------- | :----- |
| `keyCode`       | `number` | 按键码。         | `37`   |
| `timeDifference`| `number` | 时间差（毫秒）。 | `0`    |
| `status`        | `number` | 按键状态：<br/>`1`: 按下<br/>`0`: 释放 | `1`    |

### 返回值
**类型:** `Promise<void>`

**描述:** 返回一个 `Promise`，该 `Promise` 在操作成功时解析为 `void`。

### 使用示例
```js
async function setMacroData(data, macros) {
  try {
    console.log('setMacro', data, macros);
    const result = await ServiceKeyboard.setMacro(data, macros);
    return result;
  } catch (error) {
    console.error('设置宏失败:', error);
    throw error;
  }
}

// 示例：设置宏
const exampleData = {
    "index": 2,
    "len": 4,
    "mode": 0,  // 单击执行
    "key": 16,
    "num": 3,
    "delay": 4
};

const exampleMacros = [
    {
        "keyCode": 37,
        "timeDifference": 0,
        "status": 1  // 按下
    },
    {
        "keyCode": 37,
        "timeDifference": 0,
        "status": 0  // 释放
    },
    {
        "keyCode": 14,
        "timeDifference": 71.2,
        "status": 1  // 按下
    },
    {
        "keyCode": 14,
        "timeDifference": 174.3,
        "status": 0  // 释放
    }
];

// setMacroData(exampleData, exampleMacros);
```

### 注意事项

::: tip
*   `param`: 宏模式设置，需要包含完整的 `IMacroMode` 结构。
*   `macros`: 宏序列数组，每个元素需要包含 `keyCode`、`timeDifference` 和 `status`。
*   `mode`: 宏执行模式，决定了宏的执行方式：
    *   `0`: 单击执行 - 按下按键时执行一次宏序列
    *   `1`: 点击重复执行 - 按下按键时开始重复执行，再次按下时停止
    *   `2`: 按下重复执行 - 按住按键时重复执行，松开时立即停止
    *   `3`: 按下重复执行 - 按住按键时重复执行，松开后完成当前宏序列后停止
:::

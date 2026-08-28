# 宏

## 获取宏空间信息
ServiceKeyboard.getMacroSpaceInfo()

**简要描述:**
获取宏的空间信息，包括宏数量和最多支持的宏动作数量。

### 参数
该方法无需参数。

### 返回值
**类型:** `Promise<{ macroCount: number, macroNumber: number }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含宏的空间信息。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `macroCount` | `number` | 宏数量。          | `16`    |
| `macroNumber` | `number` | 最多支持的宏动作数量。     | `960`    |

### 使用示例
```js
async function getMacroSpaceInfo() {
  try {
    const result = await ServiceKeyboard.getMacroSpaceInfo();
    console.log('宏空间信息:', result);
    return result;
  } catch (error) {
    console.error('获取宏空间信息失败:', error);
    throw error;
  }
}

// 示例：获取宏空间信息
// getMacroSpaceInfo();
```

### 注意事项

::: tip
*   此接口需要协议版本 1.1.4.0 或更高版本才可用。
*   `macroCount`: 表示宏的总数量。
*   `macroNumber`: 表示最多支持的宏动作数量。
:::

## 获取宏模式
ServiceKeyboard.getMacroMode()

**简要描述:**
获取指定宏ID的宏模式设置。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含宏ID的对象。                                                                                    | 是       | 无     |
| `params.macroId` | `number` | 要查询的宏ID。                                                                                    | 是       | 无     |

### 返回值
**类型:** `Promise<{ macroId: number, mode: number, valid: boolean, actNum: number, repNum: number }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含宏的模式设置信息。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `macroId` | `number` | 宏ID。          | `1`    |
| `mode`   | `number` | 宏执行模式。     | `0`    |
| `valid`  | `boolean` | 宏有效标志。    | `true` |
| `actNum` | `number` | 宏动作数量。     | `6`    |
| `repNum` | `number` | 重复次数。       | `1`    |

### 使用示例
```js
async function getMacroModeSettings(macroId) {
  try {
    const result = await ServiceKeyboard.getMacroMode({ macroId });
    console.log(`宏ID ${macroId} 的模式设置:`, result);
    return result;
  } catch (error) {
    console.error('获取宏模式设置失败:', error);
    throw error;
  }
}

// 示例：获取宏ID为1的宏模式设置
// getMacroModeSettings(1);
```

### 注意事项

::: tip
*   `macroId`: 要查询的宏ID。
*   `mode` 表示宏的执行模式：
    *   0: 点击执行，重复n次，执行期间再次点击无效
    *   1: 点击执行，重复n次，执行期间再次点击，重新执行
    *   2: 点击执行，重复n次，执行期间再次点击，立即停止执行
    *   3: 点击执行，重复n次，执行期间再次点击，完成本次宏后停止执行
    *   4: 按住执行，重复次数无限，松开后立即停止执行
    *   5: 按住执行，重复次数无限，松开后完成本次宏后停止执行
*   `valid`: 表示该宏是否有效。
*   `actNum`: 表示宏中包含的动作数量。
*   `repNum`: 表示宏的重复执行次数。
:::

## 设置宏模式
ServiceKeyboard.setMacroMode()

**简要描述:**
设置指定宏ID的宏模式配置。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含宏模式设置的对象。                                                                              | 是       | 无     |
| `params.macroId` | `number` | 要设置的宏ID。                                                                                    | 是       | 无     |
| `params.mode` | `number` | 宏执行模式。                                                                                      | 是       | 无     |
| `params.valid` | `number` | 宏有效标志（1表示有效，0表示无效）。                                                              | 是       | 无     |
| `params.actNum` | `number` | 宏动作数量。                                                                                      | 是       | 无     |
| `params.repNum` | `number` | 重复次数。                                                                                        | 是       | 无     |

### 返回值
**类型:** `Promise<{ macroId: number, mode: number, valid: boolean, actNum: number, repNum: number }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含设置后的宏模式信息。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `macroId` | `number` | 宏ID。          | `0`    |
| `mode`   | `number` | 宏执行模式。     | `0`    |
| `valid`  | `boolean` | 宏有效标志。    | `true` |
| `actNum` | `number` | 宏动作数量。     | `8`    |
| `repNum` | `number` | 重复次数。       | `0`    |

### 使用示例
```js
async function setMacroModeSettings(macroId, mode, valid, actNum, repNum) {
  try {
    const params = {
      macroId,
      mode,
      valid,
      actNum,
      repNum
    };
    
    const result = await ServiceKeyboard.setMacroMode(params);
    console.log(`宏ID ${macroId} 的模式设置已更新:`, result);
    return result;
  } catch (error) {
    console.error('设置宏模式失败:', error);
    throw error;
  }
}

// 示例：设置宏ID为0的宏模式
// setMacroModeSettings(0, 0, 1, 8, 0);
```

### 注意事项

::: tip
*   `macroId`: 要设置的宏ID。
*   `mode` 表示宏的执行模式：
    *   0: 点击执行，重复n次，执行期间再次点击无效
    *   1: 点击执行，重复n次，执行期间再次点击，重新执行
    *   2: 点击执行，重复n次，执行期间再次点击，立即停止执行
    *   3: 点击执行，重复n次，执行期间再次点击，完成本次宏后停止执行
    *   4: 按住执行，重复次数无限，松开后立即停止执行
    *   5: 按住执行，重复次数无限，松开后完成本次宏后停止执行
*   `valid`: 输入时使用数字（1表示有效，0表示无效），返回时转换为布尔值。
*   `actNum`: 表示宏中包含的动作数量。
*   `repNum`: 表示宏的重复执行次数，0表示不重复。
:::


## 获取宏数据
ServiceKeyboard.getMacroData()

**简要描述:**
获取指定宏ID的宏动作数据。由于宏动作数据可能较多，接口支持分页获取。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含宏ID和偏移量的对象。                                                                            | 是       | 无     |
| `params.macroId` | `number` | 要查询的宏ID。                                                                                    | 是       | 无     |
| `params.offset` | `number` | 分页偏移量，用于计算分页。                                    | 是       | 无     |

### 返回值
**类型:** `Promise<{ macroId: number, offset: number, macros: Array<{ status: number, delay: number, keyCode: number }> }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含宏的动作数据信息。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `macroId` | `number` | 宏ID。          | `0`    |
| `offset` | `number` | 分页偏移量。     | `0`    |
| `macros` | `Array` | 宏动作数组。     | -      |

**macros 数组中的对象结构:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `status` | `number` | 按键状态：1=按下，0=弹起。 | `1`    |
| `delay`  | `number` | 动作执行后的延时，单位为ms。 | `47`   |
| `keyCode`| `number` | 按键码。         | `22`   |

### 使用示例
```js
async function getMacroDataSettings(macroId, offset) {
  try {
    const params = {
      macroId,
      offset
    };
    
    const result = await ServiceKeyboard.getMacroData(params);
    console.log(`宏ID ${macroId} 的动作数据:`, result);
    return result;
  } catch (error) {
    console.error('获取宏数据失败:', error);
    throw error;
  }
}

// 示例：获取宏ID为0的第一页动作数据
// getMacroDataSettings(0, 0);
```

### 注意事项

::: tip
*   `macroId`: 要查询的宏ID。
*   `offset`: 分页偏移量，用于分页获取宏动作数据。
*   `macros` 数组中的每个对象表示一个宏动作：
    *   `status`: 1表示按下，0表示弹起。
    *   `delay`: 表示该动作执行后的延时时间，单位为ms。
    *   `keyCode`: 表示要触发的按键码。
*   返回的 `macros` 数组长度固定为15，未使用的动作位置会被填充为 `{ status: 0, delay: 0, keyCode: 0 }`。
:::

## 设置宏数据
ServiceKeyboard.setMacroData()

**简要描述:**
设置指定宏ID的宏动作数据。由于宏动作数据可能较多，接口支持分页设置。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含宏ID、偏移量和动作数组的对象。                                                                  | 是       | 无     |
| `params.macroId` | `number` | 要设置的宏ID。                                                                                    | 是       | 无     |
| `params.offset` | `number` | 分页偏移量，用于计算分页。                                    | 是       | 无     |
| `params.actions` | `Array` | 宏动作数组。                                                                                      | 是       | 无     |

**actions 数组中的对象结构:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `status` | `number` | 按键状态：1=按下，0=弹起。 | `1`    |
| `delay`  | `number` | 动作执行后的延时，单位为ms。 | `119`  |
| `keyCode`| `number` | 按键码。         | `7`    |

### 返回值
**类型:** `Promise<Array<{ macroId: number, offset: number, macros: Array<{ status: number, delay: number, keyCode: number }> }>>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个数组，包含设置后的宏动作数据信息。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `macroId` | `number` | 宏ID。          | `0`    |
| `offset` | `number` | 分页偏移量。     | `0`    |
| `macros` | `Array` | 宏动作数组。     | -      |

**macros 数组中的对象结构:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `status` | `number` | 按键状态：1=按下，0=弹起。 | `1`    |
| `delay`  | `number` | 动作执行后的延时，单位为ms。 | `119`  |
| `keyCode`| `number` | 按键码。         | `7`    |

### 使用示例
```js
async function setMacroDataSettings(macroId, offset, actions) {
  try {
    const params = {
      macroId,
      offset,
      actions
    };
    
    const result = await ServiceKeyboard.setMacroData(params);
    console.log(`宏ID ${macroId} 的动作数据已更新:`, result);
    return result;
  } catch (error) {
    console.error('设置宏数据失败:', error);
    throw error;
  }
}

// 示例：设置宏ID为0的第一页动作数据
const exampleActions = [
    {
        "status": 1,
        "delay": 119,
        "keyCode": 7
    },
    {
        "status": 1,
        "delay": 32,
        "keyCode": 4
    }
    // ... 更多动作
];

// setMacroDataSettings(0, 0, exampleActions);
```

### 注意事项

::: tip
*   `macroId`: 要设置的宏ID。
*   `offset`: 分页偏移量，用于分页设置宏动作数据。
*   `actions` 数组中的每个对象表示一个宏动作：
    *   `status`: 1表示按下，0表示弹起。
    *   `delay`: 表示该动作执行后的延时时间，单位为ms。
    *   `keyCode`: 表示要触发的按键码。
*   返回的 `macros` 数组长度固定为15，未使用的动作位置会被填充为 `{ status: 0, delay: 0, keyCode: 0 }`。
*   设置成功后，返回的数据结构与 `getMacroData` 接口返回的数据结构一致。
:::

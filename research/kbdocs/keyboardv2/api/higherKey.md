# 高级按键功能

## 获取高级键类型
ServiceKeyboard.getHigherKey()

**简要描述:**
获取指定按键位置的高级键类型。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含按键位置信息的对象。                                                                             | 是       | 无     |
| `params.row` | `number` | 按键在键盘矩阵中的行号。                                                                           | 是       | 无     |
| `params.col` | `number` | 按键在键盘矩阵中的列号。                                                                           | 是       | 无     |

### 返回值
**类型:** `Promise<{ row: number, col: number, mode: number }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含按键位置和高级键类型信息。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `row`    | `number` | 按键的行号。     | `5`    |
| `col`    | `number` | 按键的列号。     | `2`    |
| `mode`   | `number` | 高级键类型。     | `1`    |

**mode 值说明:**
- `0`: 没有高级键
- `1`: DKS（动态键程）
- `2`: MPT（多点触发）
- `3`: MT（模式切换/触发延迟）
- `4`: TGL（触发延迟）
- `5`: END（结束触发）
- `6`: SOCD（同时按下冲突解决）
- `7`: RS（迅洁）

### 使用示例
```js
async function getHigherKeyType(row, col) {
  try {
    const result = await ServiceKeyboard.getHigherKey({ row, col });
    console.log(`按键位置 (${row}, ${col}) 的高级键类型:`, result);
    
    // 根据 mode 值判断高级键类型
    const modeMap = {
      0: '无高级键',
      1: 'DKS',
      2: 'MPT',
      3: 'MT',
      4: 'TGL',
      5: 'END',
      6: 'SOCD',
      7: 'RS'
    };
    
    console.log(`高级键类型: ${modeMap[result.mode]}`);
  } catch (error) {
    console.error('获取高级键类型失败:', error);
  }
}

// 示例：获取位置为 (5, 0) 的按键的高级键类型
// getHigherKeyType(5, 0);
```

### 注意事项

::: tip
*   `row` 和 `col` 表示按键在键盘矩阵中的位置。
*   `mode` 值表示该按键配置的高级键类型，可用于判断后续需要调用哪个高级键相关的接口。
:::

## 删除高级键
ServiceKeyboard.setHigherKeyNONE()

**简要描述:**
删除指定按键位置的高级键设置。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含按键位置和删除设置的对象。                                                                       | 是       | 无     |
| `params.row` | `number` | 按键在键盘矩阵中的行号。                                                                           | 是       | 无     |
| `params.col` | `number` | 按键在键盘矩阵中的列号。                                                                           | 是       | 无     |
| `params.mode` | `string` | 设置为 'NONE' 表示删除高级键。                                                                      | 是       | 无     |
| `params.data` | `object` | 包含删除设置的数据对象。                                                                           | 是       | 无     |
| `params.data.mode` | `number` | 设置为 0 表示删除高级键。                                                                          | 是       | 无     |

### 返回值
**类型:** `Promise<void>`

**描述:** 返回一个 `Promise`，该 `Promise` 在操作成功时解析为 `void`。

### 使用示例
```js
async function deleteHigherKey(row, col) {
  try {
    const params = {
      row,
      col,
      mode: 'NONE',
      data: {
        mode: 0
      }
    };
    
    await ServiceKeyboard.setHigherKeyNONE(params);
    console.log(`按键位置 (${row}, ${col}) 的高级键设置已删除`);
  } catch (error) {
    console.error('删除高级键设置失败:', error);
  }
}

// 示例：删除位置为 (5, 0) 的按键的高级键设置
// deleteHigherKey(5, 0);
```

### 注意事项

::: tip
*   `row` 和 `col` 表示要删除高级键设置的按键位置。
*   `mode` 必须设置为 'NONE'。
*   `data.mode` 必须设置为 0。
*   此操作会删除该按键的所有高级键设置。
:::

## 设置DKS高级键
ServiceKeyboard.setHigherKeyDKS()

**简要描述:**
为指定按键位置设置DKS（动态键程）高级键配置。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含按键位置和DKS设置的对象。                                                                        | 是       | 无     |
| `params.row` | `number` | 按键在键盘矩阵中的行号。                                                                           | 是       | 无     |
| `params.col` | `number` | 按键在键盘矩阵中的列号。                                                                           | 是       | 无     |
| `params.data` | `object` | 包含DKS配置的数据对象。                                                                           | 是       | 无     |
| `params.data.kcs` | `number[]` | DKS各层级的键值数组，长度为4。                                                                     | 是       | 无     |
| `params.data.trps` | `number[]` | DKS各层级的触发点行程参数数组，长度为4。每个值代表8个比特：低四位（0-3位）表示逐渐按下，高四位（4-7位）表示逐渐松开。按到底和刚抬起是同一个点位。例如31（二进制：0001 1111）表示低四位为1111，高四位为0001。 | 是       | 无     |
| `params.data.dbs` | `number[]` | DKS的死区设置数组，长度为2，单位为mm。                                                             | 是       | 无     |

### 返回值
**类型:** `Promise<{ row: number, col: number, mode: number, data: { kcs: number[], trps: number[], dbs: number[] } }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含设置后的DKS配置信息。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `row`    | `number` | 按键的行号。     | `4`    |
| `col`    | `number` | 按键的列号。     | `6`    |
| `mode`   | `number` | 高级键类型，固定为1（DKS）。 | `1`    |
| `data`   | `object` | DKS配置数据。    | -      |
| `data.kcs` | `number[]` | DKS各层级的键值数组。 | `[20, 26, 8, 9]` |
| `data.trps` | `number[]` | DKS各层级的触发点行程参数数组。每个值代表8个比特：低四位表示逐渐按下，高四位表示逐渐松开。 | `[31, 31, 31, 31]` |
| `data.dbs` | `number[]` | DKS的行程设置数组。第0位为最小行程，第1位为最大行程 | `[1.5, 3]` |

### 使用示例
```js
async function setDksHigherKey(row, col, kcs, trps, dbs) {
  try {
    const params = {
      row,
      col,
      data: {
        kcs,
        trps,
        dbs
      }
    };
    
    const result = await ServiceKeyboard.setHigherKeyDKS(params);
    console.log(`按键位置 (${row}, ${col}) 的DKS设置已更新:`, result);
  } catch (error) {
    console.error('设置DKS高级键失败:', error);
  }
}

// 示例：设置位置为 (4, 6) 的按键的DKS配置
// const exampleKcs = [20, 26, 8, 9];
// const exampleTrps = [31, 158, 229, 255];  // 31 = 0001 1111 (二进制)，低四位1111表示按下，高四位0001表示松开
// const exampleDbs = [1.5, 3];
// setDksHigherKey(4, 6, exampleKcs, exampleTrps, exampleDbs);
```

### 注意事项

::: tip
*   `row` 和 `col` 表示要设置DKS的按键位置。
*   `kcs` 数组必须包含4个键值，分别对应DKS的4个层级。
*   `trps` 数组必须包含4个触发点行程参数，分别对应DKS的4个层级。每个参数值代表8个比特：
    *   低四位（0-3位）：表示逐渐按下时的触发点
    *   高四位（4-7位）：表示逐渐松开时的触发点
    *   按到底和刚抬起是同一个点位
    *   例如：31（二进制：0001 1111）表示低四位为1111，高四位为0001
*   `dbs` 数组必须包含2个行程值，第0位为最小行程，第1位为最大行程，单位为mm。
*   返回的 `mode` 值固定为1，表示DKS类型。
:::

## 设置MPT高级键
ServiceKeyboard.setHigherKeyMPT()

**简要描述:**
为指定按键位置设置MPT（多点触发）高级键配置。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含按键位置和MPT设置的对象。                                                                        | 是       | 无     |
| `params.row` | `number` | 按键在键盘矩阵中的行号。                                                                           | 是       | 无     |
| `params.col` | `number` | 按键在键盘矩阵中的列号。                                                                           | 是       | 无     |
| `params.data` | `object` | 包含MPT配置的数据对象。                                                                           | 是       | 无     |
| `params.data.kcs` | `number[]` | MPT各层级的键值数组，长度为3。                                                                     | 是       | 无     |
| `params.data.dbs` | `number[]` | MPT各层级的行程设置数组，长度为3，单位为mm。                                                       | 是       | 无     |

### 返回值
**类型:** `Promise<{ row: number, col: number, mode: number, data: { kcs: number[], dbs: number[] } }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含设置后的MPT配置信息。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `row`    | `number` | 按键的行号。     | `3`    |
| `col`    | `number` | 按键的列号。     | `7`    |
| `mode`   | `number` | 高级键类型，固定为2（MPT）。 | `2`    |
| `data`   | `object` | MPT配置数据。    | -      |
| `data.kcs` | `number[]` | MPT各层级的键值数组。 | `[13, 4, 9]` |
| `data.dbs` | `number[]` | MPT各层级的行程设置数组，单位为0.001mm。 | `[500, 1000, 1500]` |

### 使用示例
```js
async function setMptHigherKey(row, col, kcs, dbs) {
  try {
    const params = {
      row,
      col,
      data: {
        kcs,
        dbs
      }
    };
    
    const result = await ServiceKeyboard.setHigherKeyMPT(params);
    console.log(`按键位置 (${row}, ${col}) 的MPT设置已更新:`, result);
  } catch (error) {
    console.error('设置MPT高级键失败:', error);
  }
}

// 示例：设置位置为 (3, 7) 的按键的MPT配置
// const exampleKcs = [13, 4, 9];
// const exampleDbs = [0.5, 1, 1.5];  // 单位为mm
// setMptHigherKey(3, 7, exampleKcs, exampleDbs);
```

### 注意事项

::: tip
*   `row` 和 `col` 表示要设置MPT的按键位置。
*   `kcs` 数组必须包含3个键值，分别对应MPT的3个层级。
*   `dbs` 数组必须包含3个行程值，分别对应MPT的3个层级，单位为mm。
*   返回的 `mode` 值固定为2，表示MPT类型。
*   返回的 `dbs` 数组中的值单位为0.001mm，例如500表示0.5mm。
:::

## 设置MT高级键
ServiceKeyboard.setHigherKeyMT()

**简要描述:**
为指定按键位置设置MT（模式切换/触发延迟）高级键配置。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含按键位置和MT设置的对象。                                                                         | 是       | 无     |
| `params.row` | `number` | 按键在键盘矩阵中的行号。                                                                           | 是       | 无     |
| `params.col` | `number` | 按键在键盘矩阵中的列号。                                                                           | 是       | 无     |
| `params.mode` | `string` | 设置为 'MT' 表示设置MT高级键。                                                                      | 是       | 无     |
| `params.data` | `object` | 包含MT配置的数据对象。                                                                           | 是       | 无     |
| `params.data.time` | `number` | 延迟时间，单位为ms。                                                                               | 是       | 无     |
| `params.data.kcs` | `number[]` | MT的键值数组，长度为2。                                                                           | 是       | 无     |

### 返回值
**类型:** `Promise<{ row: number, col: number, mode: number, data: { kcs: number[], time: number } }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含设置后的MT配置信息。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `row`    | `number` | 按键的行号。     | `3`    |
| `col`    | `number` | 按键的列号。     | `5`    |
| `mode`   | `number` | 高级键类型，固定为3（MT）。 | `3`    |
| `data`   | `object` | MT配置数据。    | -      |
| `data.kcs` | `number[]` | MT的键值数组。 | `[16, 18]` |
| `data.time` | `number` | 延迟时间，单位为ms。 | `200` |

### 使用示例
```js
async function setMtHigherKey(row, col, time, kcs) {
  try {
    const params = {
      row,
      col,
      mode: 'MT',
      data: {
        time,
        kcs
      }
    };
    
    const result = await ServiceKeyboard.setHigherKeyMT(params);
    console.log(`按键位置 (${row}, ${col}) 的MT设置已更新:`, result);
  } catch (error) {
    console.error('设置MT高级键失败:', error);
  }
}

// 示例：设置位置为 (3, 5) 的按键的MT配置
// const exampleTime = 200;  // 200ms
// const exampleKcs = [16, 18];
// setMtHigherKey(3, 5, exampleTime, exampleKcs);
```

### 注意事项

::: tip
*   `row` 和 `col` 表示要设置MT的按键位置。
*   `mode` 必须设置为 'MT'。
*   `time` 表示延迟时间，单位为ms。
*   `kcs` 数组必须包含2个键值。
*   返回的 `mode` 值固定为3，表示MT类型。
:::

## 设置TGL高级键
ServiceKeyboard.setHigherKeyTGL()

**简要描述:**
为指定按键位置设置TGL（触发延迟）高级键配置。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含按键位置和TGL设置的对象。                                                                         | 是       | 无     |
| `params.row` | `number` | 按键在键盘矩阵中的行号。                                                                           | 是       | 无     |
| `params.col` | `number` | 按键在键盘矩阵中的列号。                                                                           | 是       | 无     |
| `params.mode` | `string` | 设置为 'TGL' 表示设置TGL高级键。                                                                      | 是       | 无     |
| `params.data` | `object` | 包含TGL配置的数据对象。                                                                           | 是       | 无     |
| `params.data.kcs` | `number` | TGL的键值。                                                                                       | 是       | 无     |
| `params.data.delay` | `number` | 延迟时间，单位为ms。                                                                               | 是       | 无     |

### 返回值
**类型:** `Promise<{ row: number, col: number, mode: number, data: { kcs: number, time: number } }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含设置后的TGL配置信息。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `row`    | `number` | 按键的行号。     | `4`    |
| `col`    | `number` | 按键的列号。     | `4`    |
| `mode`   | `number` | 高级键类型，固定为4（TGL）。 | `4`    |
| `data`   | `object` | TGL配置数据。    | -      |
| `data.kcs` | `number` | TGL的键值。 | `29` |
| `data.time` | `number` | 延迟时间，单位为ms。 | `0` |

### 使用示例
```js
async function setTglHigherKey(row, col, kcs, delay) {
  try {
    const params = {
      row,
      col,
      mode: 'TGL',
      data: {
        kcs,
        delay
      }
    };
    
    const result = await ServiceKeyboard.setHigherKeyTGL(params);
    console.log(`按键位置 (${row}, ${col}) 的TGL设置已更新:`, result);
  } catch (error) {
    console.error('设置TGL高级键失败:', error);
  }
}

// 示例：设置位置为 (4, 4) 的按键的TGL配置
// const exampleKcs = 29;
// const exampleDelay = 200;  // 200ms
// setTglHigherKey(4, 4, exampleKcs, exampleDelay);
```

### 注意事项

::: tip
*   `row` 和 `col` 表示要设置TGL的按键位置。
*   `mode` 必须设置为 'TGL'。
*   `kcs` 为单个键值，不是数组。
*   `delay` 表示延迟时间，单位为ms。
*   返回的 `mode` 值固定为4，表示TGL类型。
*   返回的 `time` 字段值固定为0。
:::

## 设置END高级键
ServiceKeyboard.setHigherKeyEND()

**简要描述:**
为指定按键位置设置END（结束触发）高级键配置。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含按键位置和END设置的对象。                                                                         | 是       | 无     |
| `params.row` | `number` | 按键在键盘矩阵中的行号。                                                                           | 是       | 无     |
| `params.col` | `number` | 按键在键盘矩阵中的列号。                                                                           | 是       | 无     |
| `params.mode` | `string` | 设置为 'END' 表示设置END高级键。                                                                      | 是       | 无     |
| `params.data` | `object` | 包含END配置的数据对象。                                                                           | 是       | 无     |
| `params.data.kcs` | `number[]` | END的键值数组，长度为2。                                                                           | 是       | 无     |
| `params.data.delay` | `number` | 延迟时间，单位为ms。                                                                               | 是       | 无     |

### 返回值
**类型:** `Promise<{ row: number, col: number, mode: number, data: { kcs: number[], delay: number } }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含设置后的END配置信息。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `row`    | `number` | 按键的行号。     | `4`    |
| `col`    | `number` | 按键的列号。     | `4`    |
| `mode`   | `number` | 高级键类型，固定为5（END）。 | `5`    |
| `data`   | `object` | END配置数据。    | -      |
| `data.kcs` | `number[]` | END的键值数组。 | `[21, 22]` |
| `data.delay` | `number` | 延迟时间，单位为ms。 | `200` |

### 使用示例
```js
async function setEndHigherKey(row, col, kcs, delay) {
  try {
    const params = {
      row,
      col,
      mode: 'END',
      data: {
        kcs,
        delay
      }
    };
    
    const result = await ServiceKeyboard.setHigherKeyEND(params);
    console.log(`按键位置 (${row}, ${col}) 的END设置已更新:`, result);
  } catch (error) {
    console.error('设置END高级键失败:', error);
  }
}

// 示例：设置位置为 (4, 4) 的按键的END配置
// const exampleKcs = [21, 22];
// const exampleDelay = 200;  // 200ms
// setEndHigherKey(4, 4, exampleKcs, exampleDelay);
```

### 注意事项

::: tip
*   `row` 和 `col` 表示要设置END的按键位置。
*   `mode` 必须设置为 'END'。
*   `kcs` 数组必须包含2个键值。
*   `delay` 表示延迟时间，单位为ms。
*   返回的 `mode` 值固定为5，表示END类型。
:::

## 设置SOCD高级键
ServiceKeyboard.setHigherKeySOCD()

**简要描述:**
为指定按键位置设置SOCD（同时按下冲突解决）高级键配置。SOCD功能为绑定两个按键，后按下的按键会强制释放前一个按键，同一时刻只会触发一个按键。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含按键位置和SOCD设置的对象。                                                                       | 是       | 无     |
| `params.row` | `number` | 第一个按键在键盘矩阵中的行号。                                                                     | 是       | 无     |
| `params.col` | `number` | 第一个按键在键盘矩阵中的列号。                                                                     | 是       | 无     |
| `params.row2` | `number` | 第二个按键在键盘矩阵中的行号。                                                                     | 是       | 无     |
| `params.col2` | `number` | 第二个按键在键盘矩阵中的列号。                                                                     | 是       | 无     |
| `params.mode` | `number` | SOCD的工作模式：0=后覆盖，1=前优先，2=后优先，3=中性（两个按键都按下时都不生效）。                       | 是       | 无     |
| `params.delay` | `number` | 延迟时间，单位为ms。                                                                               | 是       | 无     |
| `params.kcs` | `number[]` | SOCD的键值数组，长度为2。                                                                         | 是       | 无     |

### 数据示例
以下是一个完整的SOCD高级键设置数据示例：
```json
{
  "row": 4,
  "col": 3,
  "row2": 4,
  "col2": 5,
  "mode": 2,
  "delay": 12,
  "kcs": [
    60,
    25
  ]
}
```

其中，`mode`的值表示SOCD的工作模式：
*   0: 后覆盖 - 后按下的按键会覆盖先按下的按键
*   1: 前优先 - 先按下的按键优先
*   2: 后优先 - 后按下的按键优先
*   3: 中性 - 两个按键都按下时都不生效

### 使用示例
```js
async function setSocdHigherKey(row, col, row2, col2, mode, delay, kcs) {
  try {
    // mode表示SOCD的工作模式：0=后覆盖，1=前优先，2=后优先，3=中性
    const params = {
      row,
      col,
      row2,
      col2,
      mode,
      delay,
      kcs
    };
    
    const result = await ServiceKeyboard.setHigherKeySOCD(params);
    console.log(`按键位置 (${row}, ${col}) 和 (${row2}, ${col2}) 的SOCD设置已更新:`, result);
  } catch (error) {
    console.error('设置SOCD高级键失败:', error);
  }
}

// 示例：设置位置为 (3, 6) 和 (3, 7) 的按键的SOCD配置
// const exampleKcs = [16, 18];
// const exampleDelay = 0;  // 0ms
// const exampleMode = 2;  // SOCD工作模式：2=后优先
// setSocdHigherKey(3, 6, 3, 7, exampleMode, exampleDelay, exampleKcs);
```

## 设置RS高级键
ServiceKeyboard.setHigherKeyRS()

**简要描述:**
为指定按键位置设置RS（保留/自定义联动）高级键配置。该功能可为两个按键建立联动关系，并设置延迟与键值。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params` | `object` | 包含按键位置和RS设置的对象。                                                                        | 是       | 无     |
| `params.row` | `number` | 第一个按键在键盘矩阵中的行号。                                                                     | 是       | 无     |
| `params.col` | `number` | 第一个按键在键盘矩阵中的列号。                                                                     | 是       | 无     |
| `params.mode` | `string` | 固定为 'RS'，表示设置RS高级键。                                                                      | 是       | 无     |
| `params.data` | `object` | 包含RS配置的数据对象。                                                                             | 是       | 无     |
| `params.data.row2` | `number` | 第二个按键在键盘矩阵中的行号。                                                                   | 是       | 无     |
| `params.data.col2` | `number` | 第二个按键在键盘矩阵中的列号。                                                                   | 是       | 无     |
| `params.data.delay` | `number` | 延迟时间，单位为ms。                                                                             | 是       | 无     |
| `params.data.kcs` | `number[]` | 键值数组，长度为2，对应两个按键的键值。                                                           | 是       | 无     |

### 返回值
**类型:** `Promise<{ row: number, col: number, mode: number, data: { row2: number, col2: number, kcs: number[], delay: number } }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含设置后的RS配置信息。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `row`    | `number` | 第一个按键的行号。     | `3`    |
| `col`    | `number` | 第一个按键的列号。     | `1`    |
| `mode`   | `number` | 高级键类型，固定为7（RS）。 | `7`    |
| `data`   | `object` | RS配置数据。    | -      |
| `data.row2` | `number` | 第二个按键的行号。 | `3` |
| `data.col2` | `number` | 第二个按键的列号。 | `2` |
| `data.kcs` | `number[]` | 两个按键的键值数组。 | `[4, 22]` |
| `data.delay` | `number` | 延迟时间，单位为ms。 | `0` |

### 使用示例
```js
async function setRsHigherKey(row, col, data) {
  try {
    console.log('setRS', { row, col, mode: 'RS', data });
    const result = await ServiceKeyboard.setHigherKeyRS({ row, col, mode: 'RS', data });
    console.log('设置RS高级键完成:', result);
  } catch (error) {
    console.error('设置RS高级键失败:', error);
  }
}

// 示例入参：
// setRsHigherKey(3, 1, {
//   row2: 3,
//   col2: 2,
//   delay: 0,
//   kcs: [4, 22]
// });
```

### 注意事项

:::: tip
* `row`/`col` 表示第一个按键位置，`row2`/`col2` 表示第二个按键位置。
* `mode` 必须设置为 'RS'。
* `kcs` 必须为长度为 2 的数组。
* `delay` 为延迟时间，单位为 ms。
* 返回的 `mode` 值为 7，表示 RS 类型。
::::
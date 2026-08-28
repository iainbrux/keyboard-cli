# 高级按键功能

## 获取按键的所有DKS设置(完整的DKS获取流程在这里看)
ServiceKeyboard.getDksAll()

**简要描述:**
获取指定按键在所有DKS类型（例如DKS1, DKS2, DKS3, DKS4）下配置的键值。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `key`    | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

### 返回值
**类型:** `Promise<object>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含该按键在四个DKS层级下分别设置的键值。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `dks1`   | `number` | DKS层级1的键值。 | `5`  |
| `dks2`   | `number` | DKS层级2的键值。 | `6`  |
| `dks3`   | `number` | DKS层级3的键值。 | `7`  |
| `dks4`   | `number` | DKS层级4的键值。 | `8`  |

### 使用示例
```js
async function fetchAllDksValues(targetKeyValue) {
  try {
    const dksSettings = await ServiceKeyboard.getDksAll(targetKeyValue);
    console.log(`按键 ${targetKeyValue} 的所有DKS设置:`, dksSettings);
    // console.log(`DKS1: ${dksSettings.dks1}, DKS2: ${dksSettings.dks2}, ...`);
  } catch (error) {
    console.error('获取所有DKS设置失败:', error);
  }
}

// 完整DKS数据获取示例
async function getCompleteDksData(keyValue, keyboardItem) {
  try {
    // 获取行程值
    const db = await ServiceKeyboard.getDksTravel(keyValue, 'Layout_DB1');
    const db2 = await ServiceKeyboard.getDksTravel(keyValue, 'Layout_DB3');
    
    // 获取所有DKS键值
    const dks = await ServiceKeyboard.getDksAll(keyValue);
    
    // 获取所有触发点行程参数
    const trps = await ServiceKeyboard.getTrpsAll(keyValue);
    
    // 整合所有数据
    const data = {
      keyValue,
      type: 'dks',
      dks: { ...dks },
      trps: { ...trps },
      db,
      db2
    };
    
    // 示例返回数据:
    // {
    //   "keyValue": 9,
    //   "type": "dks",
    //   "dks": {
    //     "dks1": 5,
    //     "dks2": 6,
    //     "dks3": 8,
    //     "dks4": 9
    //   },
    //   "trps": {
    //     "trps1": 63,
    //     "trps2": 63,
    //     "trps3": 127,
    //     "trps4": 127
    //   },
    //   "db": 1.4,
    //   "db2": 3
    // }
    
    console.log('获取的完整DKS数据:', data);
    return data;
  } catch (error) {
    console.error('获取完整DKS数据失败:', error);
    throw error;
  }
}

// 示例：获取 keyValue 为 80 的按键的所有DKS键值
// const exampleKey = 80;
// fetchAllDksValues(exampleKey);
```
### 注意事项

::: tip
*   `key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   完整的DKS数据获取流程包括：
    *   获取行程值（DB1和DB2）
    *   获取所有DKS键值（DKS1-DKS4）
    *   获取所有触发点行程参数（TRPS1-TRPS4）
*   `trps` 值表示触发点的行程参数（例如：63需转成二进制）
*   `db` 和 `db2` 表示行程值，通常以mm为单位（例如：1.4表示1.4mm）
:::



## 获取DKS行程值
ServiceKeyboard.getDksTravel()

**简要描述:**
获取指定按键在特定DKS类型下的行程值。行程值用于定义按键在不同触发点之间的过渡区域。

### 参数
| 参数名称  | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :-------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `key`     | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `type`    | `string` | DKS类型。例如 `'Layout_DB1'`, `'Layout_DB3'`。                                                  | 是       | 无     |

### 返回值
**类型:** `Promise<number>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个数字，表示指定类型的行程值。值通常以mm为单位（例如：1.4表示1.4mm）。

### 使用示例
```js
async function fetchDksTravelValue(targetKeyValue, dbType) {
  try {
    const dbValue = await ServiceKeyboard.getDksTravel(targetKeyValue, dbType);
    console.log(`按键 ${targetKeyValue} 的 ${dbType} 行程值为: ${dbValue}`);
  } catch (error) {
    console.error('获取DKS行程值失败:', error);
  }
}

// 示例：获取 keyValue 为 9 的按键的 DB1 行程值
// const exampleKey = 9;
// fetchDksTravelValue(exampleKey, 'Layout_DB1');
```

### 注意事项

::: tip
*   `key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `type`: 指定要查询的DKS类型，目前支持 `Layout_DB1` 和 `Layout_DB3`。
*   返回值以0.1mm为单位，例如：1.4表示0.14mm。
:::



## 获取DKS指定触发点（TRPS）的行程参数

ServiceKeyboard.getTrps()

**简要描述:**
获取指定按键在特定DKS触发点类型（TRPS）下的行程参数值。

---

### 参数 

| 参数名称         | 类型             | 描述                                                                 | 是否必需 | 默认值 |
|------------------|------------------|----------------------------------------------------------------------|----------|--------|
| `key`            | `number`         | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `type`           | `TrpsLayoutType` | DKS触发点的布局类型。具体类型定义请[查看TrpsLayoutType的模型](/keyboard/model#高级键)。 | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<{ trps: number }>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个包含指定DKS触发点行程参数的对象。
*   **解析对象结构:**

| 字段名称 | 类型     | 描述                           | 示例值  |
|----------|----------|--------------------------------|---------------|
| `trps`   | `number` | 指定触发点类型下的行程参数值。     | `150` (示例)  |

---

### 使用示例 

```js
// 假设 TrpsLayoutType 是一个已定义的枚举或类型
// enum TrpsLayoutType { TRPS1, TRPS2, ... }

async function fetchTrpsValue(targetKeyValue: number, trpsType: TrpsLayoutType) {
  try {
    const result = await ServiceKeyboard.getTrps(targetKeyValue, trpsType);
    console.log(`按键 ${targetKeyValue} 在 TRPS 类型 ${trpsType} 下的行程参数为: ${result.trps}`);
  } catch (error) {
    console.error('获取TRPS行程参数失败:', error);
  }
}

// 示例：获取 keyValue 为 80 的按键在 TRPS1 (假设类型值为0) 的行程参数
// const exampleKey = 80;
// const exampleTrpsType = TrpsLayoutType.TRPS1; // 或对应的数值
// fetchTrpsValue(exampleKey, exampleTrpsType);
```

---

### 注意事项 

::: tip
*   `key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `type`: 指定要查询的DKS触发点的具体类型，这决定了是获取哪个触发点的行程设置。
:::



## 获取按键的所有DKS触发点（TRPS）行程参数

ServiceKeyboard.getTrpsAll()

**简要描述:**
获取指定按键所有DKS触发点（例如TRPS1, TRPS2, TRPS3, TRPS4）的行程参数值。

---

### 参数 

| 参数名称 | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|----------|----------|----------------------------------------------------------------------|----------|--------|
| `key`    | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<{ trps1: number, trps2: number, trps3: number, trps4: number }>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含该按键在四个DKS触发点层级下分别设置的行程参数值。
*   **解析对象结构:**

| 字段名称 | 类型     | 描述                 | 示例值  |
|----------|----------|----------------------|---------------|
| `trps1`  | `number` | TRPS层级1的行程参数。 | `50`          |
| `trps2`  | `number` | TRPS层级2的行程参数。 | `100`         |
| `trps3`  | `number` | TRPS层级3的行程参数。 | `150`         |
| `trps4`  | `number` | TRPS层级4的行程参数。 | `200`         |

---

### 使用示例 

```js
async function fetchAllTrpsValues(targetKeyValue: number) {
  try {
    const trpsSettings = await ServiceKeyboard.getTrpsAll(targetKeyValue);
    console.log(`按键 ${targetKeyValue} 的所有TRPS行程参数:`, trpsSettings);
    // console.log(`TRPS1: ${trpsSettings.trps1}, TRPS2: ${trpsSettings.trps2}, ...`);
  } catch (error) {
    console.error('获取所有TRPS行程参数失败:', error);
  }
}

// 示例：获取 keyValue 为 80 的按键的所有TRPS行程参数
// const exampleKey = 80;
// fetchAllTrpsValues(exampleKey);
```

---

### 注意事项 

::: tip
*   `key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
:::



## 设置DKS（动态键程）相关数据

ServiceKeyboard.setDKS()    

**简要描述:**
为指定按键设置DKS（动态键程）相关的数据，包括各DKS层级的键值、对应的触发点行程TRPS。单个按键实现四种功能。

---

### 参数 

| 参数名称     | 类型       | 描述                                                                 | 是否必需 | 默认值 |
|--------------|------------|----------------------------------------------------------------------|----------|--------|
| `params`     | `object`   | 一个包含DKS设置参数的对象。                                              | 是       | 无     |
| `params.key` | `number`   | 要设置的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `params.dks` | `number[]` | 一个包含各DKS层级键值的数组，通常长度为4 (对应DKS1-DKS4)。                 | 是       | 无     |
| `params.trps`| `number[]` | 一个包含各DKS层级对应触发点行程参数的数组，通常长度为4。                  | 是       | 无     |
| `params.dbs` | `number[]` | 一个包含与DKS相关的死区或延迟设置的数组，通常长度为2 ([`db`*1000,`db2`*1000],需要把db和db2的值乘1000)。 | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<{ dks: number }>` (原始文档指定此类型，可能仅返回第一个DKS键值或一个状态码，实际返回内容需确认)
*   **描述:** 返回一个 `Promise`。成功时，可能解析为一个包含主DKS键值（例如dks1）的对象或一个表示操作状态的简单对象。具体返回内容建议通过实际测试或查阅更详细的SDK文档来确认。

---

### 使用示例 

```js
async function applyDksSettings(targetKeyValue: number, dksValues: number[], trpsValues: number[], dbsValues: number[]) {
  try {
    const params = {
      key: targetKeyValue,
      dks: dksValues,    // 例如 [5, 6, 8, 9]
      trps: trpsValues,  // 例如 [63, 100, 150, 200]
      dbs: dbsValues     // 例如 [1400, 3000]
    };
    const result = await ServiceKeyboard.setDKS(params);
    console.log(`按键 ${targetKeyValue} DKS数据已设置，返回结果:`, result);
  } catch (error) {
    console.error('设置DKS数据失败:', error);
  }
}

// 示例：为 keyValue 为 80 的按键设置DKS数据
// const exampleKey = 80;
// const dksArray = [4, 5, 6, 7]; // A, B, C, D
// const trpsArray = [40, 80, 120, 160]; 
// const dbsArray = [1.4*1000, 3*1000]; // 示例死区值
// applyDksSettings(exampleKey, dksArray, trpsArray, dbsArray);
```

---

### 注意事项 

::: tip
*   `params.key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `params.dks`: 数组，通常按顺序对应DKS1到DKS4的键值。
*   `params.trps`: 数组，通常按顺序对应DKS1到DKS4的触发行程。
*   `params.dbs`: 数组，触发的行程值。
*   返回值的 `{ dks: number }` 
:::



## 获取MPT（多点触发）设置

ServiceKeyboard.getMpt()

**简要描述:**
获取指定按键的MPT（Multi-Point Trigger）设置，实现单个按键可在三个不同的深度触发三个不同的按键。

---

### 参数 

| 参数名称 | 类型     | 描述                                                                 | 是否必需 | 默认值 |
|----------|----------|----------------------------------------------------------------------|----------|--------|
| `key`    | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

---

### 返回值 

*   **总体类型:** `Promise<{ dks: [number, number, number], dbs: [number, number, number] }>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含MPT相关的DKS键值数组和DBS参数数组。
*   **解析对象结构:**

| 字段名称 | 类型                       | 描述                                         | 示例值        |
|----------|----------------------------|----------------------------------------------|-----------------------|
| `dks`    | `[number, number, number]` | 一个包含三个MPT DKS层级键值的数组/固定长度数组。 | `[4, 7, 8]` 未选键值为0    |
| `dbs`    | `[number, number, number]` | 一个包含三个与MPT DKS层级相关的DBS触发深度参数(单位:mm)的数组。   | `[0.56, 1, 1.5]`           |

---

### 使用示例 

```js
async function fetchMptSettings(targetKeyValue: number) {
  try {
    const mptSettings = await ServiceKeyboard.getMpt(targetKeyValue);
    console.log(`按键 ${targetKeyValue} 的MPT设置:`, mptSettings);
    console.log(`MPT DKS值: ${mptSettings.dks[0]}, ${mptSettings.dks[1]}, ${mptSettings.dks[2]}`);
    console.log(`MPT DBS值: ${mptSettings.dbs[0]}, ${mptSettings.dbs[1]}, ${mptSettings.dbs[2]}`);
  } catch (error) {
    console.error('获取MPT设置失败:', error);
  }
}

// 示例：获取 keyValue 为 80 的按键的MPT设置
// const exampleKey = 80;
// fetchMptSettings(exampleKey);
```

---

### 注意事项 

::: tip
*   `key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   MPT通常允许在不同的键设置不同触发点，`dks` 数组包含这些键值，`dbs` 数组包含相关参数。
:::



## 设置MPT（多点触发）数据

ServiceKeyboard.setMpt()

**简要描述:**
为指定按键设置MPT（Multi-Point Trigger）数据，实现单个按键可在三个不同的深度触发三个不同的按键。

---

### 参数 

| 参数名称     | 类型                        | 描述                                                                 | 是否必需 | 默认值 |
|--------------|-----------------------------|----------------------------------------------------------------------|----------|--------|
| `params`     | `object`                    | 一个包含MPT设置参数的对象。                                              | 是       | 无     |
| `params.key` | `number`                    | 要设置的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `params.dks` | `number[]` (可选, 最大长度3) | 一个可选的数组，包含要设置的MPT DKS键值。如果提供，通常长度为3。         | 否       | `undefined` |
| `params.dbs` | `number[]` (可选, 最大长度3) | 一个可选的数组，包含要设置的MPT DBS参数。如果提供，通常长度为3。         | 否       | `undefined` |

---

### 返回值 

*   **总体类型:** `Promise<{ dks: [number, number, number], dbs: [number, number, number] }>`
*   **描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，表示已成功应用的MPT设置。结构与 `getMpt` 的返回值相同。
*   **解析对象结构:** (同 `getMpt` 返回值结构缺少key)

---

### 使用示例 

```js
async function applyMptSettings(targetKeyValue: number, dksValues?: number[], dbsValues?: number[]) {
  try {
    const params: { key: number; dks?: number[]; dbs?: number[] } = { key: targetKeyValue };
    if (dksValues) params.dks = dksValues;   // 例如 [101, 102, 103]
    if (dbsValues) params.dbs = dbsValues;   // 例如 [5, 6, 7]

    const result = await ServiceKeyboard.setMpt(params);
    console.log(`按键 ${targetKeyValue} MPT数据已设置，返回结果:`, result);
  } catch (error) {
    console.error('设置MPT数据失败:', error);
  }
}

// 示例：为 keyValue 为 80 的按键设置MPT数据
// const exampleKey = 80;
// const mptDks = [65, 66, 67]; // A, B, C
// const mptDbs = [4, 5, 6];
// applyMptSettings(exampleKey, mptDks, mptDbs);
// applyMptSettings(exampleKey, [68, 69, 70]); // 只更新 dks
```

---

### 注意事项 

::: tip
*   `params.key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `params.dks` 和 `params.dbs` 是可选的，允许部分更新。如果只提供 `dks`，则可能只更新键值部分，反之亦然。
:::


## 获取按键的模式切换/触发延迟（MtorTgl）设置

ServiceKeyboard.getMtorTgl()

**简要描述:**
获取指定按键的模式切换或触发延迟（MtorTgl）参数。功能为单击按键开启持续触发，按住按键为正常触发。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `key`    | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

### 返回值
**类型:** `Promise<number>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个数字，代表指定按键的MtorTgl（模式切换/触发延迟）参数值。单位为ms。

### 使用示例
```js
// 完整MT数据获取示例
async function getCompleteMtData(keyValue, keyboardItem) {
  try {
    // 获取MT延迟值
    const result = await ServiceKeyboard.getMtorTgl(keyValue);
    // 获取所有DKS键值
    const dksAll = await ServiceKeyboard.getDksAll(keyValue);
    
    console.log('getMt', result);
    console.log('getMtdksAll', dksAll);
    
    // 整合所有数据
    const data = {
      keyValue,
      type: 'mt',
      mt: {
        delay: result,
        dksAll
      }
    };
    
    // 示例返回数据:
    // {
    //   "keyValue": 17,
    //   "type": "mt",
    //   "mt": {
    //     "delay": 200,  // 200ms
    //     "dksAll": {
    //       "dks1": 6,
    //       "dks2": 8,
    //       "dks3": 20,
    //       "dks4": 0
    //     }
    //   }
    // }
    
    return data;
  } catch (error) {
    console.error('获取MT数据失败:', error);
    throw error;
  }
}

// 示例：获取 keyValue 为 17 的按键的MT数据
// const exampleKey = 17;
// getCompleteMtData(exampleKey);
```

### 注意事项

::: tip
*   `key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `delay` 值以10ms为单位，例如：200表示200ms。
:::



## 设置MT（模式切换/触发延迟）数据
ServiceKeyboard.setMT()

**简要描述:**
为指定按键设置MT（模式切换/触发延迟）数据，包括延迟时间和DKS键值。

### 参数
| 参数名称     | 类型       | 描述                                                                                                | 是否必需 | 默认值 |
| :----------- | :--------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params`     | `object`   | 一个包含MT设置参数的对象。                                                                           | 是       | 无     |
| `params.key` | `number`   | 要设置的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `params.delay`| `number`  | 延迟时间，单位为10ms。例如：20表示200ms。                                                           | 是       | 无     |
| `params.dks` | `number[]` | 一个包含DKS键值的数组，通常长度为2。                                                                 | 是       | 无     |

### 返回值
**类型:** `Promise<object>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，表示已成功应用的MT设置。

### 使用示例
```js
async function setMtData(keyValue, delay, dksValues) {
  try {
    const params = {
      key: keyValue,
      delay: delay / 10,  // 转换为10ms单位
      dks: dksValues
    };
    
    console.log('setMT', params);
    const result = await ServiceKeyboard.setMT(params);
    return result;
  } catch (error) {
    console.error('设置MT数据失败:', error);
    throw error;
  }
}

// 示例：为 keyValue 为 17 的按键设置MT数据
// const exampleKey = 17;
// const exampleDelay = 200;  // 2000ms
// const exampleDks = [6, 8];
// setMtData(exampleKey, exampleDelay, exampleDks);
```

### 注意事项

::: tip
*   `params.key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `params.delay`: 延迟时间需要除以10来转换为正确的单位（10ms）。
*   `params.dks`: 数组，包含两个DKS键值。
:::



## 获取TGL（触发延迟）设置

ServiceKeyboard.getTGL()

**简要描述:**
获取指定按键的TGL（触发延迟）设置，实现单击按键开启持续触发，按住按键为正常触发。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `key`    | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

### 返回值
**类型:** `Promise<{ key: number, delay: number }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含按键的TGL设置。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `key`    | `number` | 按键的keyValue。 | `80`   |
| `delay`  | `number` | 延迟时间，单位为ms。 | `20` (表示20ms) |

### 使用示例
```js
async function getTglSettings(keyValue) {
  try {
    const result = await ServiceKeyboard.getTGL(keyValue);
    console.log(`按键 ${keyValue} 的TGL设置:`, result);
  } catch (error) {
    console.error('获取TGL设置失败:', error);
  }
}

// 示例：获取 keyValue 为 80 的按键的TGL设置
// const exampleKey = 80;
// getTglSettings(exampleKey);
```

### 注意事项

::: tip
*   `key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
:::


## 设置TGL（触发延迟）数据

ServiceKeyboard.setTGL()

**简要描述:**
为指定按键设置TGL（触发延迟）数据，实现单击按键开启持续触发，按住按键为正常触发。

### 参数
| 参数名称     | 类型       | 描述                                                                                                | 是否必需 | 默认值 |
| :----------- | :--------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params`     | `object`   | 一个包含TGL设置参数的对象。                                                                           | 是       | 无     |
| `params.key` | `number`   | 要设置的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `params.dks` | `number[]` | 一个包含DKS键值的数组，最大长度为2。                                                                  | 否       | 无     |
| `params.delay`| `number`  | 延迟时间，单位为ms。例如：20表示20ms。                                                           | 否       | 无     |

### 返回值
**类型:** `Promise<{ key: number, delay: number }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，表示已成功应用的TGL设置。

### 使用示例
```js
async function setTglData(keyValue, dksValues, delay) {
  try {
    const params = {
      key: keyValue,
      dks: dksValues,
      delay: delay   
    };
    
    const result = await ServiceKeyboard.setTGL(params);
    console.log(`按键 ${keyValue} TGL数据已设置，返回结果:`, result);
    return result;
  } catch (error) {
    console.error('设置TGL数据失败:', error);
    throw error;
  }
}

// 示例：为 keyValue 为 80 的按键设置TGL数据
// const exampleKey = 80;
// const exampleDks = [6, 8];
// const exampleDelay = 200;  // 200ms
// setTglData(exampleKey, exampleDks, exampleDelay);
```

### 注意事项

::: tip
*   `params.key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `params.dks`: 数组，最大长度为2，包含DKS键值。
:::



## 获取END（结束触发）设置

ServiceKeyboard.getEND()

**简要描述:**
获取指定按键的END（结束触发）设置。END功能为单个按键可在松开时发送另一个按键。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `key`    | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

### 返回值
**类型:** `Promise<{ dks: number, delay: number }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含按键的END设置。

**内容:**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `dks`    | `number` | DKS键值。        | `6`    |
| `delay`  | `number` | 延迟时间，单位为ms。 | `200` (表示200ms) |

### 使用示例
```js
async function getEndSettings(keyValue) {
  try {
    const result = await ServiceKeyboard.getEND(keyValue);
    console.log(`按键 ${keyValue} 的END设置:`, result);
  } catch (error) {
    console.error('获取END设置失败:', error);
  }
}

// 示例：获取 keyValue 为 80 的按键的END设置
// const exampleKey = 80;
// getEndSettings(exampleKey);
```

### 注意事项

::: tip
*   `key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `delay` 值以ms为单位，例如：200 表示200ms。
:::



## 设置END（结束触发）数据

ServiceKeyboard.setEND()

**简要描述:**
为指定按键设置END（结束触发）数据。实现单个按键可在松开时发送另一个按键。

### 参数
| 参数名称     | 类型       | 描述                                                                                                | 是否必需 | 默认值 |
| :----------- | :--------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params`     | `object`   | 一个包含END设置参数的对象。                                                                           | 是       | 无     |
| `params.key` | `number`   | 要设置的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `params.dks` | `number` | DKS键值，                                                                  | 是       | 无     |
| `params.delay`  | `number` | 延迟时间，单位为ms。 | `200` (表示200ms) |                                                                                         | 是       | 无     |
| `version`    | `string`   | 固件版本。                                                                                          | 是       | 无     |

### 返回值
**类型:** `Promise<{ dks: number, delay: number }>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，表示已成功应用的END设置。

### 使用示例
```js
async function setEndData(keyValue, dksValues, version) {
  try {
    const params = {
      key: keyValue,
      dks: dksValues
      delay:delay
    };
    
    const result = await ServiceKeyboard.setEND(params, version);
    console.log(`按键 ${keyValue} END数据已设置，返回结果:`, result);
    return result;
  } catch (error) {
    console.error('设置END数据失败:', error);
    throw error;
  }
}

// 示例：为 keyValue 为 80 的按键设置END数据
// const exampleKey = 80;
// const exampleDks = 6;
// const exampleDelay = 200;
// const exampleVersion = await ServiceKeyboard.getApi({ type: 'ORDER_TYPE_PROTOCOL_VERSION' });
// setEndData(exampleKey, exampleDks, exampleDelay,exampleVersion);
```

### 注意事项

::: tip
*   `params.key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `version`: 固件版本号。可从接口[ServiceKeyboard.getApi](/keyboard/api/info.md#获取设备api信息)拿到
:::



## 获取SOCD（同时按下冲突解决）设置

ServiceKeyboard.getSocd()

**简要描述:**
获取指定按键的SOCD（同时按下冲突解决）设置。功能为绑定两个按键，后按下的按键会强制释放前一个按键，同一时刻只会触发一个按键。

### 参数
| 参数名称  | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :-------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `key`     | `number` | 要查询的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |
| `version` | `string` | 固件版本。                                                                                          | 是       | 无     |

### 返回值
**类型:** `Promise<object>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，包含按键的SOCD设置。根据固件版本返回不同的数据结构。

**内容 (version: 1.0.7):**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `pos1`   | `number` | 原键值1          | `1`    |
| `pos2`   | `number` | 原键值1        | `2`    |
| `key1`   | `number` | 发送键值1          | `65`   |
| `key2`   | `number` | 发送键值2          | `66`   |
| `type`   | `number` | 发送键值类型 0=按pos发送键值，1=按Key发送键值，默认:0           | `1`    |
| `mode`   | `number` | 发送键值模式 mode表示键值的发送模式，共有四种模式：0=后覆盖，1=a优先，2=b优先，3=中性（两个按键都按下都不生效）,默认:0           | `0`    |
| `delay`  | `number` | 延迟时间，单位为ms。 | `200` (表示200ms) |

**内容 (其他版本):**
| 字段名称 | 类型     | 描述             | 示例值 |
| :------- | :------- | :--------------- | :----- |
| `pos`    | `number` | 位置。           | `1`    |
| `key`    | `number` | 按键。           | `65`   |
| `type`   | `number` | 类型。           | `1`    |
| `mode`   | `number` | 模式。           | `0`    |

### 使用示例
```js
async function getSocdSettings(keyValue, version) {
  try {
    const result = await ServiceKeyboard.getSocd(keyValue, version);
    console.log(`按键 ${keyValue} 的SOCD设置:`, result);
  } catch (error) {
    console.error('获取SOCD设置失败:', error);
  }
}

// 示例：获取 keyValue 为 80 的按键的SOCD设置
// const exampleKey = 80;
// const exampleVersion = '1.0.7';
// getSocdSettings(exampleKey, exampleVersion);
```

### 注意事项

::: tip
*   `key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   `version`: 固件版本号，影响返回的数据结构。可从接口[ServiceKeyboard.getApi](/keyboard/api/info.md#获取设备api信息)拿到
*   对于版本1.0.7及以上，返回包含 `pos1`、`pos2`、`key1`、`key2` 等字段的完整数据结构。
*   对于其他版本，返回简化的数据结构，只包含 `pos`、`key` 等基本字段。
:::



## 设置SOCD（同时按下冲突解决）数据

ServiceKeyboard.setSocd()

**简要描述:**
为指定按键设置SOCD（同时按下冲突解决）数据。实现绑定两个按键，后按下的按键会强制释放前一个按键，同一时刻只会触发一个按键。

### 参数
| 参数名称     | 类型       | 描述                                                                                                | 是否必需 | 默认值 |
| :----------- | :--------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `params`     | `object`   | 一个包含SOCD设置参数的对象。                                                                          | 是       | 无     |
| `params.pos1`| `number`   | 原键值1	                                                                                             | 是       | 无     |
| `params.pos2`| `number`   | 原键值2	                                                                                             | 是       | 无     |
| `params.key1`| `number`   | 发送键值	1。                                                                                             | 是       | 无     |
| `params.key2`| `number`   | 发送键值	2。                                                                                             | 是       | 无     |
| `params.type`| `number`   | 发送键值类型 0=按pos发送键值，1=按Key发送键值，默认:0                                                                   | 是       | 无     |
| `params.mode`| `number`   | 发送键值模式 mode表示键值的发送模式，共有四种模式：0=后覆盖，1=a优先，2=b优先，3=中性（两个按键都按下都不生效）,默认:0                                                                 | 是       | 无     |
| `version`    | `string`   | 固件版本。                                                                                          | 是       | 无     |

### 返回值
**类型:** `Promise<object>`

**描述:** 返回一个 `Promise`，该 `Promise` 解析为一个对象，表示已成功应用的SOCD设置。

### 使用示例
```js
async function setSocdData(params, version) {
  try {
    const result = await ServiceKeyboard.setSocd(params, version);
    console.log('SOCD数据已设置，返回结果:', result);
    return result;
  } catch (error) {
    console.error('设置SOCD数据失败:', error);
    throw error;
  }
}

// 示例：设置SOCD数据
// const exampleParams = {
//   pos1: 1,
//   pos2: 2,
//   key1: 65,
//   key2: 66,
//   type: 1,
//   mode: 0
// };
// const exampleVersion = '1.0.7';
// setSocdData(exampleParams, exampleVersion);
```

### 注意事项

::: tip
*   `params`: 包含SOCD设置的所有必要参数。
*   `version`: 固件版本号。可从接口[ServiceKeyboard.getApi](/keyboard/api/info.md#获取设备api信息)拿到
*   参数的具体含义和取值范围请参考 [ISOCDMode、ISOCDModeV2、ISOCDModeV3的模型](/keyboard/model#高级键)。
:::



## 删除高级键

ServiceKeyboard.deleteKey()

**简要描述:**
删除指定按键的高级功能设置。

### 参数
| 参数名称 | 类型     | 描述                                                                                                | 是否必需 | 默认值 |
| :------- | :------- | :-------------------------------------------------------------------------------------------------- | :------- | :----- |
| `key`    | `number` | 要删除的按键的 `keyValue`。此值通常来自 `ServiceKeyboard.defKey()` 返回的 `IDefKeyInfo` 对象中的 `keyValue` 属性。 | 是       | 无     |

### 返回值
**类型:** `Promise<void>`

**描述:** 返回一个 `Promise`，该 `Promise` 在操作成功时解析为 `void`。

### 使用示例
```js
async function deleteAdvancedKey(keyValue) {
  try {
    await ServiceKeyboard.deleteKey(keyValue);
    console.log(`按键 ${keyValue} 的高级功能设置已删除`);
  } catch (error) {
    console.error('删除高级键失败:', error);
  }
}

// 示例：删除 keyValue 为 80 的按键的高级功能设置
// const exampleKey = 80;
// deleteAdvancedKey(exampleKey);
```

### 注意事项

::: tip
*   `key`: 指的是通过 `ServiceKeyboard.defKey()` 获取到的 `IDefKeyInfo` 对象中的 `keyValue` 属性。
*   此操作会删除该按键的所有高级功能设置，包括DKS、MPT、MT、TGL、END、SOCD等。
:::

# 全局设置

模块作用：读取和写入鼠标全局配置。调用入口为 `client.globalSetting`。

## 恢复出厂设置

client.globalSetting.factoryReset(callback?)

**简要描述:**
恢复出厂设置。

---

### 参数

| 参数       | 类型                                                              | 是否必需 | 说明                                                                   |
| ---------- | ----------------------------------------------------------------- | ---- | ---------------------------------------------------------------------- |
| `callback` | `(status: { status: 'pending' \| 'success' \| 'error' }) => void` | 否   | 状态回调。发送前回调 `pending`，成功回调 `success`，异常回调 `error`。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 成功为 `true`，失败为 `false`。

---

### 使用示例

**调用方式:**

```ts
const ok = await client.globalSetting.factoryReset(callback);
```

**调用示例:**

```ts
const ok = await client.globalSetting.factoryReset(({ status }) => {
  console.log('factory reset status:', status);
});
```

## 读取配置档位列表和当前配置索引

client.globalSetting.getConfigs()

**简要描述:**
读取配置档位列表和当前配置索引。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureConfigParam>`

**返回字段:**

| 字段         | 类型       | 说明                                         |
| ------------ | ---------- | -------------------------------------------- |
| `total`      | `number`   | 配置总数。                                   |
| `currentIdx` | `number`   | 当前配置索引。                               |
| `configs`    | `string[]` | 配置名称列表。每个名称协议侧按 32 字节处理。 |

---

### 使用示例

**调用方式:**

```ts
const configs = await client.globalSetting.getConfigs();
```

**调用示例:**

```ts
const configs = await client.globalSetting.getConfigs();
console.log(configs.currentIdx, configs.configs);
```

## 写入配置档位列表和当前配置索引

client.globalSetting.setConfigs(param)

**简要描述:**
写入配置档位列表和当前配置索引。

---

### 参数

| 参数               | 类型       | 是否必需 | 说明                                                              |
| ------------------ | ---------- | ---- | ----------------------------------------------------------------- |
| `param.total`      | `number`   | 是   | 配置总数。                                                        |
| `param.currentIdx` | `number`   | 是   | 当前配置索引。                                                    |
| `param.configs`    | `string[]` | 是   | 配置名称列表。写入时单个名称会按 UTF-8 截断到 32 字节以内并补零。 |

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureConfigParam>`
* **描述:** 返回设备确认后的配置数据。

---

### 使用示例

**调用方式:**

```ts
const result = await client.globalSetting.setConfigs(param);
```

**调用示例:**

```ts
const result = await client.globalSetting.setConfigs({
  total: 4,
  currentIdx: 0,
  configs: ['Default', 'FPS', 'Office', 'Game'],
});
```

## 读取 DPI 设置

client.globalSetting.getDPI()

**简要描述:**
读取 DPI 设置。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureDPIParam>`

**返回字段:**

| 字段                  | 类型       | 说明                  |
| --------------------- | ---------- | --------------------- |
| `currentDPI`          | `number`   | 当前 DPI。            |
| `maxDPI`              | `number`   | 最大 DPI。            |
| `maxDPISteps`         | `number`   | 最大 DPI 档位数。     |
| `dpiStepCount`        | `number`   | 当前有效 DPI 档位数。 |
| `dpiValues`           | `number[]` | DPI 档位值。          |
| `currentDPIStepIndex` | `number`   | 当前 DPI 档位索引。   |

---

### 使用示例

**调用方式:**

```ts
const dpi = await client.globalSetting.getDPI();
```

**调用示例:**

```ts
const dpi = await client.globalSetting.getDPI();
console.log(dpi.currentDPI, dpi.dpiValues);
```

## 写入 DPI 设置

client.globalSetting.setDPI(param)

**简要描述:**
写入 DPI 设置。

---

### 参数

| 参数                        | 类型       | 是否必需 | 说明                                       |
| --------------------------- | ---------- | ---- | ------------------------------------------ |
| `param.currentDPI`          | `number`   | 是   | 当前 DPI。                                 |
| `param.maxDPI`              | `number`   | 是   | 最大 DPI。                                 |
| `param.maxDPISteps`         | `number`   | 是   | 最大 DPI 档位数。                          |
| `param.dpiStepCount`        | `number`   | 是   | 有效 DPI 档位数。                          |
| `param.currentDPIStepIndex` | `number`   | 是   | 当前 DPI 档位索引。                        |
| `param.dpiValues`           | `number[]` | 是   | DPI 档位值。长度应与 `dpiStepCount` 一致。 |

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureDPIParam>`
* **描述:** 返回设备确认后的 DPI 设置。

---

### 使用示例

**调用方式:**

```ts
const result = await client.globalSetting.setDPI(param);
```

**调用示例:**

```ts
const dpi = await client.globalSetting.getDPI();

const result = await client.globalSetting.setDPI({
  ...dpi,
  currentDPI: 1600,
  currentDPIStepIndex: 1,
  dpiValues: [800, 1600, 3200, 6400],
  dpiStepCount: 4,
});
```

## 读取回报率设置

client.globalSetting.getReportSettings()

**简要描述:**
读取回报率设置。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureReportParam>`

**返回字段:**

| 字段          | 类型                              | 说明                                        |
| ------------- | --------------------------------- | ------------------------------------------- |
| `reportTotal` | `number`                          | 支持的回报率数量。                          |
| `reportIndex` | `number`                          | 当前回报率在 `reports` 中的索引。           |
| `reports`     | `GlobalFeatureRateOfReturnType[]` | 支持的回报率枚举键，例如 `R1KHz`、`R8KHz`。 |

---

### 使用示例

**调用方式:**

```ts
const report = await client.globalSetting.getReportSettings();
```

**调用示例:**

```ts
const report = await client.globalSetting.getReportSettings();
console.log(report.reportIndex, report.reports);
```

## 写入当前回报率索引

client.globalSetting.setReportSettings(reportIndex)

**简要描述:**
写入当前回报率索引。

---

### 参数

| 参数          | 类型     | 是否必需 | 说明                           |
| ------------- | -------- | ---- | ------------------------------ |
| `reportIndex` | `number` | 是   | 回报率索引，不是 Hz 数值本身。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用方式:**

```ts
const ok = await client.globalSetting.setReportSettings(reportIndex);
```

**调用示例:**

```ts
const report = await client.globalSetting.getReportSettings();
const nextIndex = report.reports.indexOf('R1KHz');

if (nextIndex >= 0) {
  await client.globalSetting.setReportSettings(nextIndex);
}
```

## 读取按键扫描率设置

client.globalSetting.getKeyPressScan()

**简要描述:**
读取按键扫描率设置。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureKeyPressScanParam>`

**返回字段:**

| 字段                 | 类型       | 说明                   |
| -------------------- | ---------- | ---------------------- |
| `currentScanRate`    | `number`   | 当前按键扫描率。       |
| `supportedScanRates` | `number[]` | 支持的按键扫描率列表。 |

---

### 使用示例

**调用方式:**

```ts
const scan = await client.globalSetting.getKeyPressScan();
```

**调用示例:**

```ts
const scan = await client.globalSetting.getKeyPressScan();
console.log(scan.currentScanRate, scan.supportedScanRates);
```

## 写入按键扫描率

client.globalSetting.setKeyPressScan(scanRate)

**简要描述:**
写入按键扫描率。

---

### 参数

| 参数       | 类型     | 是否必需 | 说明                                                                                  |
| ---------- | -------- | ---- | ------------------------------------------------------------------------------------- |
| `scanRate` | `number` | 是   | 要写入的按键扫描率。建议使用 `getKeyPressScan()` 返回的 `supportedScanRates` 中的值。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用方式:**

```ts
const ok = await client.globalSetting.setKeyPressScan(scanRate);
```

**调用示例:**

```ts
const scan = await client.globalSetting.getKeyPressScan();
await client.globalSetting.setKeyPressScan(scan.supportedScanRates[0]);
```

## 读取传感器扫描率设置

client.globalSetting.getSensorScan()

**简要描述:**
读取传感器扫描率设置。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureSensorScanParam>`

**返回字段:**

| 字段                 | 类型       | 说明                     |
| -------------------- | ---------- | ------------------------ |
| `currentScanRate`    | `number`   | 当前传感器扫描率。       |
| `supportedScanRates` | `number[]` | 支持的传感器扫描率列表。 |

---

### 使用示例

**调用方式:**

```ts
const scan = await client.globalSetting.getSensorScan();
```

**调用示例:**

```ts
const scan = await client.globalSetting.getSensorScan();
console.log(scan.currentScanRate, scan.supportedScanRates);
```

## 写入传感器扫描率

client.globalSetting.setSensorScan(scanRate)

**简要描述:**
写入传感器扫描率。

---

### 参数

| 参数       | 类型     | 是否必需 | 说明                                                                                  |
| ---------- | -------- | ---- | ------------------------------------------------------------------------------------- |
| `scanRate` | `number` | 是   | 要写入的传感器扫描率。建议使用 `getSensorScan()` 返回的 `supportedScanRates` 中的值。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用方式:**

```ts
const ok = await client.globalSetting.setSensorScan(scanRate);
```

**调用示例:**

```ts
const scan = await client.globalSetting.getSensorScan();
await client.globalSetting.setSensorScan(scan.supportedScanRates[0]);
```

## 读取性能模式

client.globalSetting.getPerformanceMode()

**简要描述:**
读取性能模式。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeaturePerformanceModeParam>`

**返回字段:**

| 字段   | 类型     | 说明             |
| ------ | -------- | ---------------- |
| `mode` | `number` | 当前性能模式值。 |

---

### 使用示例

**调用方式:**

```ts
const performance = await client.globalSetting.getPerformanceMode();
```

**调用示例:**

```ts
const performance = await client.globalSetting.getPerformanceMode();
console.log(performance.mode);
```

## 写入性能模式

client.globalSetting.setPerformanceMode(mode)

**简要描述:**
写入性能模式。

---

### 参数

| 参数   | 类型     | 是否必需 | 说明         |
| ------ | -------- | ---- | ------------ |
| `mode` | `number` | 是   | 性能模式值。 |

---

### 返回值

* **总体类型:** `Promise<GlobalFeaturePerformanceModeParam>`
* **描述:** 返回设备确认后的模式。

---

### 使用示例

**调用方式:**

```ts
const result = await client.globalSetting.setPerformanceMode(mode);
```

**调用示例:**

```ts
const result = await client.globalSetting.setPerformanceMode(1);
console.log(result.mode);
```

## 读取睡眠时间设置

client.globalSetting.getSleepTime()

**简要描述:**
读取睡眠时间设置。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureSleepTimeParam>`

**返回字段:**

| 字段         | 类型     | 说明             |
| ------------ | -------- | ---------------- |
| `sleepTime1` | `number` | 第一段睡眠时间。 |
| `sleepTime2` | `number` | 第二段睡眠时间。 |

---

### 使用示例

**调用方式:**

```ts
const sleep = await client.globalSetting.getSleepTime();
```

**调用示例:**

```ts
const sleep = await client.globalSetting.getSleepTime();
console.log(sleep.sleepTime1, sleep.sleepTime2);
```

## 写入睡眠时间设置

client.globalSetting.setSleepTime(param)

**简要描述:**
写入睡眠时间设置。

---

### 参数

| 参数               | 类型     | 是否必需 | 说明             |
| ------------------ | -------- | ---- | ---------------- |
| `param.sleepTime1` | `number` | 是   | 第一段睡眠时间。 |
| `param.sleepTime2` | `number` | 是   | 第二段睡眠时间。 |

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureSleepTimeParam>`
* **描述:** 返回设备确认后的睡眠时间设置。

---

### 使用示例

**调用方式:**

```ts
const result = await client.globalSetting.setSleepTime(param);
```

**调用示例:**

```ts
await client.globalSetting.setSleepTime({
  sleepTime1: 60,
  sleepTime2: 300,
});
```

## 读取按键消抖时间

client.globalSetting.getKeyDebounceTime()

**简要描述:**
读取按键消抖时间。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureKeyDebounceTimeParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `debounceTime` | `number` | 当前按键消抖时间，单位 ms。 |

---

### 使用示例

**调用示例:**

```ts
const debounce = await client.globalSetting.getKeyDebounceTime();
console.log(debounce.debounceTime);
```

## 写入按键消抖时间

client.globalSetting.setKeyDebounceTime(debounceTime)

**简要描述:**
写入按键消抖时间。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `debounceTime` | `number` | 是 | 按键消抖时间，单位 ms。0~10ms |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用示例:**

```ts
const ok = await client.globalSetting.setKeyDebounceTime(5);
```

## 读取 LOD 高度档位

client.globalSetting.getLODHeight()

**简要描述:**
读取 LOD 高度档位。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureLODHeightParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `lodHeight` | `number` | 当前 LOD 档位。 |

---

### 使用示例

**调用示例:**

```ts
const lod = await client.globalSetting.getLODHeight();
console.log(lod.lodHeight);
```

## 写入 LOD 高度档位

client.globalSetting.setLODHeight(lodHeight)

**简要描述:**
写入 LOD 高度档位。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `lodHeight` | `number` | 是 | LOD 档位: `0` 表示0.7mm，`1` 表示1mm, `2` 表示2mm。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用示例:**

```ts
const ok = await client.globalSetting.setLODHeight(1);
```

## 读取直线修正模式

client.globalSetting.getStraightLineCorrection()

**简要描述:**
读取直线修正模式。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureStraightLineCorrectionParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `mode` | `number` | 当前状态，`0` 表示关闭，`1` 表示开启。 |

---

### 使用示例

**调用示例:**

```ts
const correction = await client.globalSetting.getStraightLineCorrection();
console.log(correction.mode);
```

## 写入直线修正模式

client.globalSetting.setStraightLineCorrection(mode)

**简要描述:**
写入直线修正模式。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `mode` | `number` | 是 | `0` 表示关闭，`1` 表示开启。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用示例:**

```ts
const ok = await client.globalSetting.setStraightLineCorrection(1);
```

## 读取当前电量百分比

client.globalSetting.getBattery()

**简要描述:**
读取当前电量百分比。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureBatteryParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `batteryPercentage` | `number` | 当前电量百分比。 |

---

### 使用示例

**调用示例:**

```ts
const battery = await client.globalSetting.getBattery();
console.log(battery.batteryPercentage);
```

## 读取当前充电状态

client.globalSetting.getChargingStatus()

**简要描述:**
读取当前充电状态。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureChargingStatusParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `chargingStatus` | `number` | 当前充电状态，`0` 表示未充电，`1` 表示充电中。 |

---

### 使用示例

**调用方式:**

```ts
const status = await client.globalSetting.getChargingStatus();
```

**调用示例:**

```ts
const status = await client.globalSetting.getChargingStatus();
console.log(status.chargingStatus);
```

---

### 注意事项

协议功能：`GlobalFeature.ChargingStatus = 0x16`，读写位为 `Read`。

## 读取指定键位的马达震动强度档位

client.globalSetting.getMotorVibrationStrength(key)

**简要描述:**
读取指定键位的马达震动强度档位。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `key` | `number` | 是 | 要查询的键位。 |

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureMotorVibrationParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `key` | `number` | 当前查询的键位。 |
| `strengthLevel` | `number` | 当前强度档位。 |
| `supportedLevelCount` | `number` | 支持的档位数量。 |
| `triggerMode` | `number` | 触发方式，`0` 按下触发，`1` 抬起触发，`2` 按下和抬起均触发。 |

---

### 使用示例

**调用示例:**

```ts
const vibration = await client.globalSetting.getMotorVibrationStrength(1);
console.log(vibration.strengthLevel, vibration.triggerMode);
```

## 写入指定键位的马达震动强度档位

client.globalSetting.setMotorVibrationStrength(param)

**简要描述:**
写入指定键位的马达震动强度档位。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `param.key` | `number` | 是 | 要设置的键位。 |
| `param.strengthLevel` | `number` | 是 | 强度档位。 |
| `param.triggerMode` | `number` | 是 | 触发方式，`0` 按下触发，`1` 抬起触发，`2` 按下和抬起均触发。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用示例:**

```ts
const ok = await client.globalSetting.setMotorVibrationStrength({
  key: 1,
  strengthLevel: 2,
  triggerMode: 0,
});
```

## 读取 20K 高性能帧扫描模式

client.globalSetting.getHighPerformance20KScanMode()

**简要描述:**
读取 20K 高性能帧扫描模式。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureHighPerformance20KScanModeParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `mode` | `number` | 当前状态，`0` 表示关闭，`1` 表示开启。 |

---

### 使用示例

**调用示例:**

```ts
const mode = await client.globalSetting.getHighPerformance20KScanMode();
console.log(mode.mode);
```

## 写入 20K 高性能帧扫描模式

client.globalSetting.setHighPerformance20KScanMode(mode)

**简要描述:**
写入 20K 高性能帧扫描模式。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `mode` | `number` | 是 | `0` 表示关闭，`1` 表示开启。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用示例:**

```ts
const ok = await client.globalSetting.setHighPerformance20KScanMode(1);
```

## 读取传感器帧同步模式

client.globalSetting.getSensorFrameSyncMode()

**简要描述:**
读取传感器帧同步模式。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureSensorFrameSyncModeParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `mode` | `number` | 当前状态，`0` 表示关闭，`1` 表示开启。 |

---

### 使用示例

**调用示例:**

```ts
const mode = await client.globalSetting.getSensorFrameSyncMode();
console.log(mode.mode);
```

## 写入传感器帧同步模式

client.globalSetting.setSensorFrameSyncMode(mode)

**简要描述:**
写入传感器帧同步模式。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `mode` | `number` | 是 | `0` 表示关闭，`1` 表示开启。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用示例:**

```ts
const ok = await client.globalSetting.setSensorFrameSyncMode(1);
```

## 读取 DPI 档位灯效颜色

client.globalSetting.getDPILightEffect()

**简要描述:**
读取 DPI 档位灯效颜色。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureDPILightEffectParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `currentDPIStepIndex` | `number` | 当前 DPI 档位索引。 |
| `supportedDPIColorCount` | `number` | 支持的 DPI 档位颜色数量。 |
| `dpiColors` | `string[]` | DPI 档位灯效颜色列表，格式为 `#RRGGBB`。 |

---

### 使用示例

**调用示例:**

```ts
const dpiLight = await client.globalSetting.getDPILightEffect();
console.log(dpiLight.currentDPIStepIndex, dpiLight.dpiColors);
```

## 写入指定 DPI 档位的灯效颜色

client.globalSetting.setDPILightEffect(param)

**简要描述:**
写入指定 DPI 档位的灯效颜色。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `param.currentDPIStepIndex` | `number` | 是 | 要设置颜色的 DPI 档位索引。 |
| `param.dpiColor` | `string` | 是 | 要写入的颜色，格式为 `#RRGGBB`。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用示例:**

```ts
const ok = await client.globalSetting.setDPILightEffect({
  currentDPIStepIndex: 1,
  dpiColor: '#32ff39',
});
```

## 读取传感器角度

client.globalSetting.getSensorAngle()

**简要描述:**
读取传感器角度。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureSensorAngleParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `angle` | `number` | 当前传感器角度，范围按设备协议为 `-30` 到 `30` 度。 |

---

### 使用示例

**调用示例:**

```ts
const sensorAngle = await client.globalSetting.getSensorAngle();
console.log(sensorAngle.angle);
```

## 设置传感器角度

client.globalSetting.setSensorAngle(angle)

**简要描述:**
设置传感器角度。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `angle` | `number` | 是 | 要设置的传感器角度，范围按设备协议为 `-30` 到 `30` 度。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用示例:**

```ts
const ok = await client.globalSetting.setSensorAngle(-10);
```

## 读取射频模式

client.globalSetting.getRadioFrequencyMode()

**简要描述:**
读取射频模式。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeatureRadioFrequencyModeParam>`
**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `mode` | `number` | 当前射频模式，`0` 表示智能射频，`1` 表示满格射频。 |

---

### 使用示例

**调用方式:**

```ts
const rfMode = await client.globalSetting.getRadioFrequencyMode();
```

**调用示例:**

```ts
const rfMode = await client.globalSetting.getRadioFrequencyMode();
console.log(rfMode.mode);
```

---

### 注意事项

协议功能：`GlobalFeature.RadioFrequencyModeSetting = 0x12`，读写位为 `Read`。

## 设置射频模式

client.globalSetting.setRadioFrequencyMode(mode)

**简要描述:**
设置射频模式。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `mode` | `number` | 是 | `0` 表示智能射频，`1` 表示满格射频。 |

---

### 返回值

* **总体类型:** `Promise<boolean>`
* **描述:** 设备返回成功码时为 `true`。

---

### 使用示例

**调用方式:**

```ts
const ok = await client.globalSetting.setRadioFrequencyMode(mode);
```

**调用示例:**

```ts
const ok = await client.globalSetting.setRadioFrequencyMode(1);
```

---

### 注意事项

协议功能：`GlobalFeature.RadioFrequencyModeSetting = 0x12`，读写位为 `Write`。

## 读取省电模式

client.globalSetting.getPowerSavingMode()

**简要描述:**
读取省电模式和超级省电模式。

---

### 参数

此方法不需要参数。

---

### 返回值

* **总体类型:** `Promise<GlobalFeaturePowerSavingModeParam>`

**返回字段:**
| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `powerSavingMode` | `number` | 省电模式，`0` 表示关闭，`1` 表示开启。 |
| `superPowerSavingMode` | `number` | 超级省电模式，`0` 表示关闭，`1` 表示开启。 |

---

### 使用示例

**调用方式:**

```ts
const powerSaving = await client.globalSetting.getPowerSavingMode();
```

**调用示例:**

```ts
const powerSaving = await client.globalSetting.getPowerSavingMode();
console.log(powerSaving.powerSavingMode, powerSaving.superPowerSavingMode);
```

---

### 注意事项

协议功能：`GlobalFeature.PowerSavingModeSetting = 0x15`，读写位为 `Read`。

## 设置省电模式

client.globalSetting.setPowerSavingMode(param)

**简要描述:**
设置省电模式和超级省电模式。


🪧省电模式和超级省电模式为互斥。

---

### 参数

| 参数 | 类型 | 是否必需 | 说明 |
| --- | --- | --- | --- |
| `param.powerSavingMode` | `number` | 是 | 省电模式，`0` 表示关闭，`1` 表示开启。 |
| `param.superPowerSavingMode` | `number` | 是 | 超级省电模式，`0` 表示关闭，`1` 表示开启。 |

---

### 返回值

* **总体类型:** `Promise<number>`
* **描述:** 设备返回的状态码，`0` 表示成功。

---

### 使用示例

**调用方式:**

```ts
const status = await client.globalSetting.setPowerSavingMode(param);
```

**调用示例:**

```ts
const status = await client.globalSetting.setPowerSavingMode({
  powerSavingMode: 1,
  superPowerSavingMode: 0,
});
console.log(status);
```

---

### 注意事项

协议功能：`GlobalFeature.PowerSavingModeSetting = 0x15`，读写位为 `Write`。

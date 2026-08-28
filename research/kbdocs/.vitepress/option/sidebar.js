export default {
  '/keyboard/': [
    {
      text: '键盘sdk',
      items: [
        { text: '开始', link: '/keyboard/' },
        {
          text: 'API',
          link: '/keyboard/api/info',
          items: [
            { text: '基础的设备信息', link: '/keyboard/api/info' },
            { text: '布局/改键', link: '/keyboard/api/key' },
            { text: '灯光', link: '/keyboard/api/lighting' },
            { text: '性能', link: '/keyboard/api/performance' },
            { text: '高级键', link: '/keyboard/api/higherKey' },
            { text: '宏', link: '/keyboard/api/macro' },
          ],
        },
        { text: '模型', link: '/keyboard/model' },
        { text: '键值表', link: '/keyboard/keyboard' },
        { text: '参数类型查询', link: '/keyboard/type' },
      ],
    },
  ],
  '/keyboardv2/': [
    {
      text: '键盘sdkv2',
      items: [
        { text: '开始', link: '/keyboardv2/' },
        {
          text: 'API',
          link: '/keyboardv2/api/info',
          items: [
            { text: '基础的设备信息', link: '/keyboardv2/api/info' },
            { text: '布局/改键', link: '/keyboardv2/api/key' },
            { text: '灯光', link: '/keyboardv2/api/lighting' },
            { text: '性能', link: '/keyboardv2/api/performance' },
            { text: '高级键', link: '/keyboardv2/api/higherKey' },
            { text: '宏', link: '/keyboardv2/api/macro' },
            { text: '手柄', link: '/keyboardv2/api/handle' },
            { text: '自定义命令', link: '/keyboardv2/api/custom' },
          ],
        },
        { text: '键值表', link: '/keyboardv2/keyboard' },
      ],
    },
  ],
  '/mouse/': [
    {
      text: '鼠标sdk',
      items: [
        { text: '开始', link: '/mouse/' },
        {
          text: 'API',
          link: '/mouse/api/info',
          items: [
            { text: '基础的设备信息', link: '/mouse/api/info' },
            { text: '全局设置', link: '/mouse/api/globalSetting' },
            { text: '性能', link: '/mouse/api/performance' },
            { text: '按键映射', link: '/mouse/api/keyRemapping' },
            { text: '宏', link: '/mouse/api/macro' },
            { text: '固件升级', link: '/mouse/api/upload' },
            // { text: '运行时与协议内部模块', link: '/mouse/api/runtime' },
            // { text: '工具模块', link: '/mouse/api/utils' },
          ],
        },
        { text: '键值表', link: '/mouse/keyboard' },
      ],
    },
  ],
};

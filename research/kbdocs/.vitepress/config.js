import { defineConfig } from 'vitepress';
import sidebar from './option/sidebar.js';
import nav from './option/nav.js';

export default defineConfig({
  title: '星闪悦动',
  description: '星闪悦动',
  logo: '/logo.svg',
  lang: 'zh-CN',
  base: '/keyboard-docs/',
  lastUpdated: true,
  themeConfig: {
    socialLinks: [{ icon: 'github', link: 'https://github.com/sparklinkplayjoy/keyboard-docs' }],
    nav,
    sidebar,
    search: {
      provider: 'local',
    },
    docFooter: {
      prev: '上一页',
      next: '下一页',
    },
    lastUpdated: {
      text: '更新时间',
      formatOptions: {
        dateStyle: 'full',
        timeStyle: 'medium',
      },
    },
    outline: {
      level: [2, 3],
      label: '目录',
    },
  },
});

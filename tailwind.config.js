/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        zen: {
          bg: 'var(--zen-bg)',
          fg: 'var(--zen-fg)',
          border: 'var(--zen-border)',
          accent: 'var(--zen-accent)',
          'accent-fg': 'var(--zen-accent-fg)',
          muted: 'var(--zen-muted)',
          'muted-fg': 'var(--zen-muted-fg)',
          subtle: 'var(--zen-subtle)',
          sidebar: 'var(--zen-sidebar)',
          'sidebar-fg': 'var(--zen-sidebar-fg)',
          'sidebar-border': 'var(--zen-sidebar-border)',
        },
      },
      fontFamily: {
        mono: ['var(--zen-font-mono)', 'ui-monospace', 'SFMono-Regular', 'Menlo', 'monospace'],
        sans: ['var(--zen-font-sans)', 'ui-sans-serif', 'system-ui', 'sans-serif'],
      },
    },
  },
  plugins: [],
}

/** @type {import('tailwindcss').Config} */
module.exports = {
  mode: "all",
  content: ["./src/**/*.{rs,html,css}", "./dist/**/*.html"],
  theme: {
    colors: {
      'blue': '#1fb6ff',
      'purple': '#7e5bef',
      'pink': '#ff49db',
      'orange': '#ff7849',
      'green': '#13ce66',
      'yellow': '#ffc82c',
      'gray-dark': '#273444',
      'gray': '#8492a6',
      'gray-light': '#d3dce6',
      'violet':'#ede9fe',
      'white':'#f8fafc',
      'gray-50':'#f9fafb',
      'gray-100':'#f3f4f6',
      'gray-200':'#e5e7eb',
      'gray-300':'#d1d5db',
      'gray-400':'#9ca3af',
      'gray-500':'#6b7280',
      'gray-600':'#4b5563',
      'gray-700':'#374151',
      'gray-800':'#1f2937',
      'gray-900':'#111827',
      'gray-950':'#030712',
      'blue-50':'#eff6ff',
      'blue-100':'#dbeafe',
      'blue-200':'#bfdbfe',
      'blue-300':'#93c5fd',
      'blue-400':'#60a5fa',
      'blue-500':'#3b82f6',
      'blue-600':'#2563eb',
      'blue-700':'#1d4ed8',
      'blue-800':'#1e40af',
      'blue-900':'#1e3a8a',
      'blue-950':'#172554',
    },
    extend: {},
  },
  plugins: [
  ],
  safelist: [
    {
      pattern: /./,
    },
  ],
  variants: {
    extend: {
      textDecoration:['focus-visible'],
      ringWidth: ['focus-visible'],
      ringColor: ['focus-visible'],
      ringOffsetWidth: ['focus-visible'],
      ringOffsetColor: ['focus-visible'],
    },
  }
};
  
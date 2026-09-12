/** @type {import('tailwindcss').Config} */
export default {
    content: ['./index.html', './src/**/*.{vue,ts}'],
    theme: {
        extend: {
            fontFamily: {
                sans: ['Inter', 'Avenir', 'Helvetica', 'Arial', 'sans-serif'],
                mono: ['JetBrains Mono', 'Consolas', 'Menlo', 'monospace'],
            },
            colors: {
                ink: {
                    900: '#0B1120',
                    800: '#111A2E',
                    700: '#1B2740',
                },
                glow: {
                    cyan: '#22D3EE',
                    blue: '#0EA5E9',
                    gold: '#F5C56B',
                },
                muted: '#94A3B8',
            },
            keyframes: {
                shimmer: {
                    '0%': {transform: 'translateX(-100%)'},
                    '100%': {transform: 'translateX(100%)'},
                },
                'fade-in': {
                    '0%': {opacity: '0', transform: 'translateY(4px)'},
                    '100%': {opacity: '1', transform: 'translateY(0)'},
                },
            },
            animation: {
                shimmer: 'shimmer 1.6s infinite',
                'fade-in': 'fade-in 0.25s ease-out both',
            },
        },
    },
}

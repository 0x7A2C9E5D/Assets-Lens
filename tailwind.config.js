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
                // Every colour resolves through a CSS variable so the light palette is one
                // `[data-theme]` block away; the dark values in index.css are the ones the UI
                // shipped with, hence the default theme looks exactly as before.
                page: 'rgb(var(--c-page) / <alpha-value>)',
                ink: {
                    900: 'rgb(var(--c-ink-900) / <alpha-value>)',
                    800: 'rgb(var(--c-ink-800) / <alpha-value>)',
                    700: 'rgb(var(--c-ink-700) / <alpha-value>)',
                },
                fg: 'rgb(var(--c-fg) / <alpha-value>)',
                muted: 'rgb(var(--c-muted) / <alpha-value>)',
                // Status colours: the bright steps that read well on ink are washed out on white
                ok: 'rgb(var(--c-ok) / <alpha-value>)',
                danger: 'rgb(var(--c-danger) / <alpha-value>)',
                // Neutral hairlines and hover washes: white on dark, ink on light, so the same
                // `/5` and `/10` steps stay visible in both themes.
                line: 'rgb(var(--c-line) / <alpha-value>)',
                overlay: 'rgb(var(--c-overlay) / <alpha-value>)',
                // Ready-made steps whose *strength* is theme-specific: a 5% white hairline reads
                // fine on ink, but a 5% ink hairline all but disappears on paper. These bake the
                // alpha into the token so one class carries both themes.
                hairline: 'rgb(var(--c-line) / var(--alpha-line))',
                'hairline-strong': 'rgb(var(--c-line) / var(--alpha-line-strong))',
                tint: 'rgb(var(--c-overlay) / var(--alpha-tint))',
                'tint-strong': 'rgb(var(--c-overlay) / var(--alpha-tint-strong))',
                subtle: 'rgb(var(--c-muted) / var(--alpha-subtle))',
                faint: 'rgb(var(--c-muted) / var(--alpha-faint))',
                // Branded borders and rings that must stay legible on a light page
                edge: 'rgb(var(--c-edge) / <alpha-value>)',
                // Modal scrim: it has to darken whatever is behind it, so unlike the other tokens it
                // stays dark in both themes instead of following the page
                scrim: 'rgb(var(--c-scrim) / <alpha-value>)',
                glow: {
                    cyan: 'rgb(var(--c-glow-cyan) / <alpha-value>)',
                    blue: 'rgb(var(--c-glow-blue) / <alpha-value>)',
                    gold: 'rgb(var(--c-glow-gold) / <alpha-value>)',
                },
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

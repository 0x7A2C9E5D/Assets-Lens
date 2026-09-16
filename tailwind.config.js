/** @type {import('tailwindcss').Config} */
export default {
    content: ['./index.html', './src/**/*.{vue,ts}'],
    theme: {
        extend: {
            fontFamily: {
                // Fluent 2 ships Segoe UI Variable as two optical sizes: Text for UI copy, Display for
                // titles and large numbers. Windows 10 has neither, so both stacks fall back to Segoe UI.
                sans: [
                    '"Segoe UI Variable Text"',
                    '"Segoe UI Variable"',
                    '"Segoe UI"',
                    'system-ui',
                    'sans-serif',
                ],
                display: [
                    '"Segoe UI Variable Display"',
                    '"Segoe UI Variable"',
                    '"Segoe UI"',
                    'system-ui',
                    'sans-serif',
                ],
                // Cascadia is the Fluent mono (Windows Terminal, dev tools); Consolas is the fallback
                mono: ['"Cascadia Mono"', '"Cascadia Code"', 'Consolas', 'monospace'],
            },
            // Fluent 2 shape scale: 2 / 4 / 8 / 12. Small controls and tiles take 4px, cards and
            // containers 8px, and nothing in the UI is rounder than a dialog.
            borderRadius: {
                sm: '2px',
                DEFAULT: '4px',
                md: '4px',
                lg: '4px',
                xl: '8px',
                '2xl': '8px',
                '3xl': '12px',
            },
            // Fluent 2 motion: one easing curve (fast-out-slow-in) and two durations
            transitionTimingFunction: {
                fluent: 'cubic-bezier(0.33, 0, 0.67, 1)',
            },
            colors: {
                // Every colour resolves through a CSS variable so both themes are one `[data-theme]`
                // block apart. The names are the Fluent alias tokens the values come from.
                page: 'rgb(var(--c-page) / <alpha-value>)', // neutralBackground3 (app canvas)
                // Three neutral surfaces, darkest first: 900 is a recessed well, 800 a layer / table
                // header, 700 the card surface everything else sits on
                ink: {
                    900: 'rgb(var(--c-ink-900) / <alpha-value>)',
                    800: 'rgb(var(--c-ink-800) / <alpha-value>)',
                    700: 'rgb(var(--c-ink-700) / <alpha-value>)',
                },
                fg: 'rgb(var(--c-fg) / <alpha-value>)', // neutralForeground1
                muted: 'rgb(var(--c-muted) / <alpha-value>)', // neutralForeground3
                // Neutral strokes and hover washes: `line` and `overlay` are the raw colours, the
                // composite tokens below bake in the theme-specific alpha
                line: 'rgb(var(--c-line) / <alpha-value>)',
                overlay: 'rgb(var(--c-overlay) / <alpha-value>)',
                hairline: 'rgb(var(--c-line) / var(--alpha-line))',
                'hairline-strong': 'rgb(var(--c-line) / var(--alpha-line-strong))',
                tint: 'rgb(var(--c-overlay) / var(--alpha-tint))',
                'tint-strong': 'rgb(var(--c-overlay) / var(--alpha-tint-strong))',
                subtle: 'rgb(var(--c-muted) / var(--alpha-subtle))',
                faint: 'rgb(var(--c-muted) / var(--alpha-faint))',
                // Brand: `accent` is brandForeground1 (text, icons, focus, selection), `brand` is
                // brandBackground (the fill of a primary button)
                accent: 'rgb(var(--c-accent) / <alpha-value>)',
                brand: 'rgb(var(--c-brand) / <alpha-value>)',
                'on-brand': 'rgb(var(--c-on-brand) / <alpha-value>)',
                // Status: the foreground step, plus the tinted background / border a banner uses
                success: 'rgb(var(--c-success) / <alpha-value>)',
                'success-bg': 'rgb(var(--c-success-bg) / <alpha-value>)',
                'success-line': 'rgb(var(--c-success-line) / <alpha-value>)',
                warning: 'rgb(var(--c-warning) / <alpha-value>)',
                'warning-bg': 'rgb(var(--c-warning-bg) / <alpha-value>)',
                'warning-line': 'rgb(var(--c-warning-line) / <alpha-value>)',
                danger: 'rgb(var(--c-danger) / <alpha-value>)',
                'danger-bg': 'rgb(var(--c-danger-bg) / <alpha-value>)',
                'danger-line': 'rgb(var(--c-danger-line) / <alpha-value>)',
                // Close-hover fill of the window caption button (Windows 11 keeps it red in both themes)
                close: 'rgb(var(--c-close) / <alpha-value>)',
                // Modal scrim: unlike the other tokens it stays dark in both themes, so its alpha is
                // baked in rather than written at the call site
                veil: 'rgb(var(--c-scrim) / var(--alpha-scrim))',
            },
            keyframes: {
                'fade-in': {
                    '0%': {opacity: '0', transform: 'translateY(2px)'},
                    '100%': {opacity: '1', transform: 'translateY(0)'},
                },
            },
            animation: {
                // Fluent durationNormal; the old shimmed skeleton bar is gone with the glass look
                'fade-in': 'fade-in 0.2s cubic-bezier(0.33, 0, 0.67, 1) both',
            },
        },
    },
}

import { defineConfig } from 'vitest/config.js';

export default defineConfig({
    test: {
        include: ['tests/**/*.test.ts'],
        testTimeout: 350000,
        hookTimeout: 100000,
        reporters: ['verbose'],
    },
});

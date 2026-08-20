import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects Vite on a fixed port and serves the built assets from `dist/`.
// The strict port is important: Tauri's dev command spawns Vite on a specific
// URL and will not go looking if the port is taken.
export default defineConfig({
    plugins: [react()],
    clearScreen: false,
    server: {
        port: 1420,
        strictPort: true,
    },
    build: {
        target: "es2022",
        outDir: "dist",
        emptyOutDir: true,
    },
});

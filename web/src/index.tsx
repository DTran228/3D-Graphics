import ReactDOM from 'react-dom/client';
import * as wasm from '../wasm/wasm_graphics'

import App from './react_components/App'
import { BenchmarkPage, configurations, type Configuration } from './benchmark';

import './index.css'

const root = ReactDOM.createRoot(document.getElementById('root')!);
const hardwareThreads = navigator.hardwareConcurrency || 1;
const query = new URLSearchParams(location.search);
const benchmark = query.get('benchmark');
const configuration = benchmark && Object.hasOwn(configurations, benchmark) ? benchmark as Configuration : null;
const requested = Number(query.get('threads'));
const editorThreads = Number.isInteger(requested) && requested >= 1 && requested <= 32
    ? requested : Math.min(hardwareThreads, 8);
const threads = configuration ? configurations[configuration].threads : editorThreads;

async function initialize() {
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
        if (!crossOriginIsolated || typeof SharedArrayBuffer === 'undefined') {
            throw new Error('SharedArrayBuffer requires cross-origin isolation. Open the app from Vite on localhost or use a host with COOP/COEP headers.');
        }
        root.render(<p role="status">Loading renderer ({threads} requested workers)…</p>);
        const wasmStart = performance.now();
        await wasm.default();
        const poolStart = performance.now();
        await Promise.race([
            wasm.initThreadPool(threads),
            new Promise((_, reject) => {
                timeout = setTimeout(() => reject(new Error('Rayon workers did not initialize within 15 seconds.')), 15_000);
            }),
        ]);
        const runtime = {
            requestedThreads: threads,
            actualThreads: wasm.ray_thread_count(),
            hardwareConcurrency: hardwareThreads,
            wasmLoadMs: poolStart - wasmStart,
            poolInitMs: performance.now() - poolStart,
        };
        root.render(configuration
            ? <BenchmarkPage configuration={configuration} runtime={runtime} />
            : <App runtime={runtime} />);
    } catch (error) {
        root.render(<p role="alert">Renderer unavailable. {String(error)}</p>);
    } finally {
        clearTimeout(timeout);
    }
}

void initialize();

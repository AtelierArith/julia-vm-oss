// SubsetJuliaVM Playground - Mobile-first web playground
import { samplesIR } from './samples_ir.js?v=2';
import { registerJuliaLanguage, setWasmModule } from './julia-language.js?v=1';

// DOM Elements
const sampleSelect = document.getElementById('sample-select');
const tabs = document.querySelectorAll('.tab');
const panes = document.querySelectorAll('.pane');
const monacoContainer = document.getElementById('monaco-container');
const runBtn = document.getElementById('run-btn');
const shareBtn = document.getElementById('share-btn');
const copySourceBtn = document.getElementById('copy-source-btn');
const output = document.getElementById('output');
const result = document.getElementById('result');
const errorDiv = document.getElementById('error');
const unsupportedNotice = document.getElementById('unsupported-notice');
const versionSpan = document.getElementById('version');
const copyBtn = document.getElementById('copy-btn');
const clearOutputBtn = document.getElementById('clear-output-btn');
const plotOutput = document.getElementById('plot-output');

// State
let wasm = null;
let editor = null;
let currentSampleIndex = -1;
let warmupPromise = null;
let warmupHandle = null;
let warmupHandleType = null;
let warmupScheduled = false;

const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
const runShortcut = isMac ? '⌘+Enter' : 'Ctrl+Enter';
const runButtonText = `Run (${runShortcut})`;

// ============================================================
// URL Sharing
// ============================================================

function getCodeFromHash() {
    const hash = window.location.hash;
    if (!hash || hash.length < 3) return null;
    const params = new URLSearchParams(hash.substring(1));
    const encoded = params.get('c');
    if (!encoded) return null;
    try {
        const decoded = LZString.decompressFromEncodedURIComponent(encoded);
        if (!decoded) throw new Error('Decompression returned null');
        return decoded;
    } catch (e) {
        console.error('Failed to decode code from URL:', e);
        return null;
    }
}

function setCodeToHash(code) {
    const encoded = LZString.compressToEncodedURIComponent(code);
    const newHash = `#c=${encoded}`;
    const newUrl = `${window.location.origin}${window.location.pathname}${newHash}`;
    history.replaceState(null, '', newUrl);
    return newUrl;
}

async function copyShareUrl(code) {
    const url = setCodeToHash(code);
    try {
        await navigator.clipboard.writeText(url);
        return url;
    } catch (e) {
        console.error('Clipboard API failed:', e);
        const textArea = document.createElement('textarea');
        textArea.value = url;
        textArea.style.position = 'fixed';
        textArea.style.opacity = '0';
        document.body.appendChild(textArea);
        textArea.select();
        try {
            document.execCommand('copy');
        } catch (e2) {
            console.error('Fallback copy failed:', e2);
            throw new Error(`Could not copy automatically. URL: ${url}`);
        } finally {
            document.body.removeChild(textArea);
        };
        return url;
    }
}

// ============================================================
// Editor helpers
// ============================================================

function setCode(code) {
    if (editor) {
        editor.setValue(code);
    }
}

function getCode() {
    return editor ? editor.getValue() : '';
}

// ============================================================
// Tabs (mobile)
// ============================================================

function switchTab(name) {
    tabs.forEach((tab) => {
        const active = tab.dataset.tab === name;
        tab.classList.toggle('active', active);
        tab.setAttribute('aria-selected', String(active));
    });
    panes.forEach((pane) => {
        pane.classList.toggle('active', pane.id === `${name}-pane`);
    });
    // Monaco needs a layout refresh when its container becomes visible
    if (name === 'edit' && editor) {
        setTimeout(() => editor.layout(), 0);
    }
}

function setupTabs() {
    tabs.forEach((tab) => {
        tab.addEventListener('click', () => switchTab(tab.dataset.tab));
    });
}

// ============================================================
// Sample dropdown
// ============================================================

// Populate the <select> with one <option> per sample, grouped by category so the
// 29-entry list stays scannable. The option value is the sample's index in
// samplesIR; "custom" marks code restored from a shared URL.
function populateSampleSelect() {
    sampleSelect.innerHTML = '';

    const groups = new Map();
    samplesIR.forEach((sample, idx) => {
        if (!groups.has(sample.category)) {
            groups.set(sample.category, []);
        }
        groups.get(sample.category).push(idx);
    });

    for (const [category, indices] of groups) {
        const group = document.createElement('optgroup');
        group.label = category;
        indices.forEach((idx) => {
            const sample = samplesIR[idx];
            const opt = document.createElement('option');
            opt.value = String(idx);
            opt.textContent = sample.webUnsupported ? `${sample.name} (web unsupported)` : sample.name;
            group.appendChild(opt);
        });
        sampleSelect.appendChild(group);
    }
}

// Show a transient "Custom code" entry when the editor holds shared/edited code
// that doesn't correspond to a sample.
function selectCustomOption() {
    let opt = sampleSelect.querySelector('option[value="custom"]');
    if (!opt) {
        opt = document.createElement('option');
        opt.value = 'custom';
        opt.textContent = 'Custom code';
        sampleSelect.insertBefore(opt, sampleSelect.firstChild);
    }
    sampleSelect.value = 'custom';
}

function setupSampleSelect() {
    populateSampleSelect();
    sampleSelect.addEventListener('change', () => {
        const idx = parseInt(sampleSelect.value, 10);
        if (Number.isNaN(idx)) return;
        loadSample(idx);
        if (window.innerWidth < 768) {
            switchTab('edit');
        }
    });
}

// ============================================================
// Sample loading
// ============================================================

function loadSample(idx) {
    const sample = samplesIR[idx];
    if (!sample) return;

    setCode(sample.code);
    currentSampleIndex = idx;
    sampleSelect.value = String(idx);
    hideError();
    hideUnsupported();
    result.textContent = '';
    output.textContent = '';
    output.classList.remove('hidden');
    plotOutput.innerHTML = '';
    plotOutput.classList.add('hidden');
}

// ============================================================
// Event listeners
// ============================================================

function setupEventListeners() {
    runBtn.addEventListener('click', run);
    shareBtn.addEventListener('click', shareCode);
    copySourceBtn.addEventListener('click', copySource);
    copyBtn.addEventListener('click', copyOutput);
    clearOutputBtn.addEventListener('click', clearOutput);
}

async function copySource() {
    const text = getCode();
    if (!text) return;
    try {
        await navigator.clipboard.writeText(text);
        copySourceBtn.textContent = 'Copied!';
        copySourceBtn.classList.add('copied');
        setTimeout(() => {
            copySourceBtn.textContent = 'Copy';
            copySourceBtn.classList.remove('copied');
        }, 1500);
    } catch (e) {
        console.error('Failed to copy:', e);
    }
}

async function shareCode() {
    const code = getCode();
    if (!code) {
        showError('No code to share.');
        return;
    }
    try {
        await copyShareUrl(code);
        shareBtn.textContent = 'Copied!';
        shareBtn.classList.add('copied');
        setTimeout(() => {
            shareBtn.textContent = 'Share';
            shareBtn.classList.remove('copied');
        }, 1500);
    } catch (e) {
        showError(e.message);
    }
}

async function copyOutput() {
    const text = output.textContent;
    if (!text) return;
    try {
        await navigator.clipboard.writeText(text);
        copyBtn.textContent = 'Copied!';
        copyBtn.classList.add('copied');
        setTimeout(() => {
            copyBtn.textContent = 'Copy';
            copyBtn.classList.remove('copied');
        }, 1500);
    } catch (e) {
        console.error('Failed to copy:', e);
        const textArea = document.createElement('textarea');
        textArea.value = text;
        textArea.style.position = 'fixed';
        textArea.style.opacity = '0';
        document.body.appendChild(textArea);
        textArea.select();
        try {
            document.execCommand('copy');
            copyBtn.textContent = 'Copied!';
            copyBtn.classList.add('copied');
            setTimeout(() => {
                copyBtn.textContent = 'Copy';
                copyBtn.classList.remove('copied');
            }, 1500);
        } catch (e2) {
            console.error('Fallback copy failed:', e2);
        }
        document.body.removeChild(textArea);
    }
}

function clearOutput() {
    output.textContent = '';
    output.classList.remove('hidden');
    if (typeof Plotly !== 'undefined') {
        try { Plotly.purge(plotOutput); } catch (_) {}
    }
    plotOutput.innerHTML = '';
    plotOutput.classList.add('hidden');
    result.textContent = '';
    hideError();
    hideUnsupported();
}

// ============================================================
// WASM loading
// ============================================================

async function loadWasm() {
    try {
        const module = await import('./pkg/subset_julia_vm_web.js');
        await module.default();
        wasm = module;
        console.log('WASM module loaded successfully');
    } catch (e) {
        console.warn('WASM module not available:', e);
        output.textContent = 'WASM module not loaded. Please run:\n\n' +
            '  cd subset_julia_vm_web\n' +
            '  wasm-pack build --target web --out-dir ../web/pkg\n\n' +
            'Then refresh this page.';
    }
}

async function loadMonaco() {
    return new Promise((resolve) => {
        require.config({
            paths: {
                'vs': 'https://cdn.jsdelivr.net/npm/monaco-editor@0.45.0/min/vs'
            }
        });

        require(['vs/editor/editor.main'], function(monaco) {
            registerJuliaLanguage(monaco);

            editor = monaco.editor.create(monacoContainer, {
                value: 'println("Hello, World!")',
                language: 'julia',
                theme: 'julia-monokai',
                fontSize: 14,
                fontFamily: "'Fira Code', 'Consolas', 'Monaco', monospace",
                fontLigatures: true,
                lineNumbers: 'on',
                minimap: { enabled: false },
                scrollBeyondLastLine: false,
                automaticLayout: true,
                tabSize: 4,
                insertSpaces: true,
                wordWrap: 'on',
                renderWhitespace: 'none',
                cursorBlinking: 'smooth',
                cursorSmoothCaretAnimation: 'on',
                smoothScrolling: true,
                padding: { top: 10, bottom: 10 },
                lineHeight: 21,
                renderLineHighlight: 'line',
                scrollbar: {
                    vertical: 'auto',
                    horizontal: 'auto',
                    useShadows: false,
                    verticalScrollbarSize: 10,
                    horizontalScrollbarSize: 10
                },
                quickSuggestions: true,
                suggestOnTriggerCharacters: true,
                acceptSuggestionOnEnter: 'on',
                wordBasedSuggestions: 'off',
                suggest: {
                    snippetsPreventQuickSuggestions: false,
                    showKeywords: true,
                    showFunctions: true,
                    showVariables: true,
                    showConstants: true
                }
            });

            editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, function() {
                run();
            });

            // Re-layout when the window is resized (automaticLayout covers most,
            // but this helps on orientation changes and tab switches).
            window.addEventListener('resize', () => {
                if (editor) editor.layout();
            });

            resolve();
        });
    });
}

// ============================================================
// Run and display results
// ============================================================

// Workaround: some iOS samples depend on packages/JS renderers not shipped in the
// static web build; show a friendly fallback instead of executing. (Issue #7286)
function showUnsupported(sample) {
    unsupportedNotice.textContent = `“${sample.name}” is not supported in the web build (${sample.description}). Try it in the iOS app.`;
    unsupportedNotice.classList.remove('hidden');
}

function hideUnsupported() {
    unsupportedNotice.classList.add('hidden');
}

async function run() {
    if (!wasm) {
        showError('WASM module not loaded. Build it first with wasm-pack.');
        return;
    }

    const code = getCode();
    const sample = samplesIR[currentSampleIndex];
    const seed = 42;

    cancelScheduledWarmup();
    setCodeToHash(code);
    hideError();
    hideUnsupported();
    result.textContent = '';

    if (sample && sample.webUnsupported) {
        showUnsupported(sample);
        // Switch to output tab on mobile so the notice is visible
        if (window.innerWidth < 768) {
            switchTab('output');
        }
        return;
    }

    runBtn.disabled = true;
    runBtn.textContent = 'Running...';

    try {
        console.log('Executing with run_from_source...');
        const execResult = wasm.run_from_source(code, BigInt(seed));
        await displayResult(execResult);
    } catch (e) {
        showError(e.message || 'Execution failed');
    } finally {
        runBtn.disabled = false;
        runBtn.textContent = runButtonText;
        scheduleWarmup();
    }
}

async function displayResult(execResult) {
    plotOutput.innerHTML = '';
    plotOutput.classList.add('hidden');
    output.textContent = '';
    output.classList.remove('hidden');

    if (execResult.success) {
        const mime = execResult.artifact_mime;
        const data = execResult.artifact_data;
        if (mime === 'application/vnd.plotly+json' && data) {
            plotOutput.classList.remove('hidden');
            plotOutput.style.height = '450px';
            try {
                const parsed = JSON.parse(data);
                if (typeof Plotly !== 'undefined') {
                    const traces = parsed.traces || [];
                    const layout = themedPlotlyLayout(parsed.layout || {});
                    if (parsed.frames && parsed.frames.length) {
                        const dur = (parsed.layout && parsed.layout._frameDuration) || 50;
                        await Plotly.newPlot(plotOutput, {
                            data: traces,
                            layout: layout,
                            frames: parsed.frames,
                            config: { responsive: true }
                        });
                        Plotly.Plots.resize(plotOutput);
                        Plotly.animate(plotOutput, null, {
                            frame: { duration: dur, redraw: true },
                            transition: { duration: 0 },
                            fromcurrent: true,
                            mode: 'immediate'
                        });
                        result.textContent = 'Rendered animation';
                        if (execResult.output) {
                            output.textContent = execResult.output;
                        } else {
                            output.classList.add('hidden');
                        }
                        return;
                    }
                    await Plotly.newPlot(plotOutput, traces, layout, { responsive: true });
                    Plotly.Plots.resize(plotOutput);
                    if (plotOutput.children.length === 0) {
                        plotOutput.textContent = '[Plotly rendered no visible output]';
                    } else {
                        result.textContent = 'Rendered plot';
                    }
                } else {
                    plotOutput.textContent = '[Plotly.js not loaded - cannot render plot]';
                }
            } catch (e) {
                plotOutput.textContent = `[Plotly render error: ${e.message}]`;
            }
            if (execResult.output) {
                output.textContent = execResult.output;
            } else {
                output.classList.add('hidden');
            }
        } else if (mime === 'application/vnd.jsxgraph+json' && data) {
            plotOutput.classList.remove('hidden');
            plotOutput.style.height = '450px';
            renderJsxgraph(data);
            if (execResult.output) {
                output.textContent = execResult.output;
            } else {
                output.classList.add('hidden');
            }
        } else {
            if (execResult.output) {
                output.textContent += execResult.output;
            }
            if (execResult.value !== 0 && !isNaN(execResult.value)) {
                result.textContent = `Result: ${execResult.value}`;
            } else if (!execResult.output) {
                result.textContent = 'Completed';
            }
        }
    } else {
        if (execResult.output) {
            output.textContent += execResult.output;
        }
        showError(execResult.error_message || 'Execution failed');
    }

    // Auto-switch to output tab on mobile after running
    if (window.innerWidth < 768) {
        switchTab('output');
    }
}

// Render a JSXGraph board spec (`{"options": {...}, "elements": [...]}`) into the
// plot pane. Mirrors the iOS JSXGraphView renderer (Issue #6357 / #7286): each
// element is created via board.create(type, parents, attrs); parents reference
// earlier elements as {ref: id}. Requires the global JXG from jsxgraph.min.js.
function renderJsxgraph(data) {
    const box = document.createElement('div');
    box.id = 'jxgbox';
    box.style.width = '100%';
    box.style.height = '100%';
    box.style.aspectRatio = '1 / 1';
    plotOutput.appendChild(box);

    if (typeof JXG === 'undefined' || !JXG.JSXGraph) {
        box.textContent = '[JSXGraph.js not loaded — cannot render board]';
        return;
    }

    try {
        const spec = JSON.parse(data);
        const options = spec.options || { boundingbox: [-5, 5, 5, -5], axis: true };
        const board = JXG.JSXGraph.initBoard('jxgbox', options);
        const created = {};

        const resolveSpecValue = (p) => {
            if (p !== null && typeof p === 'object' && p.ref !== undefined) {
                return created[p.ref];
            }
            if (p !== null && typeof p === 'object' && p.jsfunc !== undefined) {
                const argv = Array.isArray(p.vars) ? p.vars : [p.var || 't'];
                return new Function(...argv, `return (${p.jsfunc});`);
            }
            if (Array.isArray(p)) {
                return p.map(resolveSpecValue);
            }
            return p;
        };

        const createElements = (container, elements) => {
            for (const el of elements || []) {
                const attrs = { ...(el.attrs || {}) };
                // Make the internal id available to JSXGraph and to ref resolution.
                attrs.id = String(el.id);
                const parents = Array.isArray(el.parents) ? el.parents.map(resolveSpecValue) : [];
                const createdElement = container.create(el.type, parents, attrs);
                created[el.id] = createdElement;
                if (el.type === 'view3d') {
                    createElements(createdElement, el.elements || []);
                }
            }
        };

        createElements(board, spec.elements || []);
        result.textContent = 'Rendered board';
    } catch (e) {
        box.textContent = `[JSXGraph render error: ${e.message}]`;
    }
}

function themedPlotlyLayout(layout) {
    const themed = {
        ...layout,
        paper_bgcolor: layout.paper_bgcolor || '#1e1f1c',
        plot_bgcolor: layout.plot_bgcolor || '#272822',
        font: { color: '#f8f8f2', ...(layout.font || {}) },
        margin: { l: 48, r: 20, t: 24, b: 48, ...(layout.margin || {}) }
    };

    themed.xaxis = themedAxis(layout.xaxis);
    themed.yaxis = themedAxis(layout.yaxis);
    if (layout.scene) {
        themed.scene = {
            ...layout.scene,
            xaxis: themedAxis(layout.scene.xaxis),
            yaxis: themedAxis(layout.scene.yaxis),
            zaxis: themedAxis(layout.scene.zaxis)
        };
    }
    return themed;
}

function themedAxis(axis = {}) {
    return {
        ...axis,
        color: axis.color || '#f8f8f2',
        gridcolor: axis.gridcolor || '#4a4a40',
        zerolinecolor: axis.zerolinecolor || '#75715e',
        linecolor: axis.linecolor || '#75715e'
    };
}

// ============================================================
// WASM warmup
// ============================================================

function warmupWasm() {
    if (!wasm) {
        return Promise.resolve();
    }
    if (warmupPromise) {
        return warmupPromise;
    }
    warmupPromise = new Promise((resolve) => {
        try {
            wasm.run_from_source('using Plots\nplot(sin)\n', BigInt(42));
            console.log('WASM plot warmup completed');
        } catch (e) {
            console.warn('WASM plot warmup failed:', e);
        } finally {
            resolve();
        }
    });
    return warmupPromise;
}

function scheduleWarmup() {
    if (!wasm || warmupPromise || warmupScheduled) {
        return;
    }

    warmupScheduled = true;
    const startWarmup = () => {
        warmupHandle = null;
        warmupHandleType = null;
        warmupScheduled = false;
        warmupWasm();
    };

    warmupHandle = window.setTimeout(() => {
        warmupHandle = null;
        warmupHandleType = null;
        if ('requestIdleCallback' in window) {
            warmupHandle = window.requestIdleCallback(startWarmup, { timeout: 3000 });
            warmupHandleType = 'idle';
        } else {
            startWarmup();
        }
    }, 2000);
    warmupHandleType = 'timeout';
}

function cancelScheduledWarmup() {
    if (!warmupScheduled || warmupHandle === null) {
        return;
    }

    if (warmupHandleType === 'idle' && 'cancelIdleCallback' in window) {
        window.cancelIdleCallback(warmupHandle);
    } else {
        window.clearTimeout(warmupHandle);
    }
    warmupHandle = null;
    warmupHandleType = null;
    warmupScheduled = false;
}

// ============================================================
// Error helpers
// ============================================================

function showError(message) {
    errorDiv.textContent = message;
    errorDiv.classList.remove('hidden');
}

function hideError() {
    errorDiv.classList.add('hidden');
}

// ============================================================
// Initialization
// ============================================================

async function init() {
    runBtn.disabled = true;
    runBtn.textContent = 'Loading...';

    setupTabs();
    setupSampleSelect();
    setupEventListeners();

    await Promise.all([loadWasm(), loadMonaco()]);

    if (wasm) {
        setWasmModule(wasm);
        versionSpan.textContent = `SubsetJuliaVM v${wasm.get_version()}`;
    }

    // Restore code from URL hash if present
    const hashCode = getCodeFromHash();
    if (hashCode !== null) {
        setCode(hashCode);
        currentSampleIndex = -1;
        selectCustomOption();
    } else if (samplesIR.length > 0) {
        loadSample(0);
    }

    if (wasm) {
        runBtn.textContent = 'Warming...';
        await warmupWasm();
    }

    runBtn.disabled = false;
    runBtn.textContent = runButtonText;
}

init();

// Debug helper
window.debugRun = function(code) {
    if (!wasm) {
        console.error('WASM not loaded');
        return null;
    }
    const result = wasm.run_from_source(code, BigInt(42));
    console.log('Result:', result);
    return result;
};

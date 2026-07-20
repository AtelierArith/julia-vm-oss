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
        const artifacts = Array.isArray(execResult.artifacts) && execResult.artifacts.length
            ? execResult.artifacts
            : (execResult.artifact_mime && execResult.artifact_data
                ? [{ mime: execResult.artifact_mime, data: execResult.artifact_data }]
                : []);
        if (artifacts.length) {
            plotOutput.classList.remove('hidden');
            plotOutput.style.height = 'auto';
            let rendered = 0;
            for (const artifact of artifacts) {
                const box = document.createElement('div');
                box.className = 'artifact-output';
                box.style.width = '100%';
                box.style.height = '450px';
                box.style.marginBottom = '12px';
                plotOutput.appendChild(box);
                if (await renderArtifactBox(box, artifact.mime, artifact.data)) {
                    rendered += 1;
                }
            }
            result.textContent = rendered === 1 ? 'Rendered artifact' : `Rendered ${rendered} artifacts`;
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

async function renderArtifactBox(target, mime, data) {
    if (mime === 'application/vnd.plotly+json' && data) {
        return renderPlotlyArtifact(target, data);
    }
    if (mime === 'application/vnd.jsxgraph+json' && data) {
        renderJsxgraph(data, target);
        return true;
    }
    target.textContent = '[Unsupported display artifact]';
    return false;
}

async function renderPlotlyArtifact(target, data) {
    try {
        const parsed = JSON.parse(data);
        if (typeof Plotly === 'undefined') {
            target.textContent = '[Plotly.js not loaded - cannot render plot]';
            return false;
        }
        const traces = parsed.traces || [];
        const layout = themedPlotlyLayout(parsed.layout || {});
        // Issue #9206: a growing-path animation (`framesCompact`) is rendered
        // progressively by growing one trace, so peak memory is O(n).
        if (parsed.framesCompact) {
            const dur = (parsed.layout && parsed.layout._frameDuration) || 50;
            await renderCompactProgressive(target, parsed.framesCompact, layout, dur);
            return true;
        }
        if (parsed.frames && parsed.frames.length) {
            const dur = (parsed.layout && parsed.layout._frameDuration) || 50;
            await Plotly.newPlot(target, {
                data: traces,
                layout: layout,
                frames: parsed.frames,
                config: { responsive: true }
            });
            Plotly.Plots.resize(target);
            Plotly.animate(target, null, {
                frame: { duration: dur, redraw: true },
                transition: { duration: 0 },
                fromcurrent: true,
                mode: 'immediate'
            });
            return true;
        }
        await Plotly.newPlot(target, traces, layout, { responsive: true });
        Plotly.Plots.resize(target);
        if (target.children.length === 0) {
            target.textContent = '[Plotly rendered no visible output]';
            return false;
        }
        return true;
    } catch (e) {
        target.textContent = `[Plotly render error: ${e.message}]`;
        return false;
    }
}

// Render a JSXGraph board spec (`{"options": {...}, "elements": [...]}`) into the
// plot pane. Mirrors the iOS JSXGraphView renderer (Issue #6357 / #7286): each
// element is created via board.create(type, parents, attrs); parents reference
// earlier elements as {ref: id}. Requires the global JXG from jsxgraph.min.js.
function renderJsxgraph(data, host = plotOutput) {
    const box = document.createElement('div');
    box.id = `jxgbox-${Date.now()}-${Math.floor(Math.random() * 1000000)}`;
    box.style.width = '100%';
    box.style.height = '100%';
    box.style.aspectRatio = '1 / 1';
    host.appendChild(box);

    if (typeof JXG === 'undefined' || !JXG.JSXGraph) {
        box.textContent = '[JSXGraph.js not loaded — cannot render board]';
        return;
    }

    try {
        const spec = JSON.parse(data);
        const options = spec.options || { boundingbox: [-5, 5, 5, -5], axis: true };
        const board = JXG.JSXGraph.initBoard(box.id, options);
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
    } catch (e) {
        box.textContent = `[JSXGraph render error: ${e.message}]`;
    }
}

// Issue #9206: render a growing-path animation (`framesCompact` =
// {full, counts[, titles]}) without materializing all N native Plotly frames
// (O(frames²) points in memory → WKWebView OOM on iOS). Points are kept at FULL
// resolution (thinning them did not help on-device and made attractors look
// ragged). One trace is grown with `extendTraces` / rewound with `restyle`, so
// peak memory is O(n). 3D (gl3d) is driven exactly like 2D (SVG): start empty and
// auto-play the growing path, full resolution, same playback.
//
// Historical note (Issue #9218 → #9237): a large Aizawa `@animate` OOM was once
// blamed on gl3d redraws leaking WebGL buffers. #9237 found the real cause was the
// Editor result-value JSON echo (typed_value_json, O(frames²)), now bounded; gl3d
// redraws are not the OOM source, so 3D needs no redraw or speed penalty.
async function renderCompactProgressive(el, fc, layout, dur) {
    const full = fc.full || [];
    const counts = fc.counts || [];
    const titles = fc.titles || null;
    const nTraces = full.length;
    const nFrames = counts.length;
    if (!nFrames || !nTraces) return;
    const has3d = full.some((tr) => Array.isArray(tr.z));
    const host = (typeof el === 'string') ? document.getElementById(el) : el;
    // Reserve a row for the control bar INSIDE the plot box (match iOS #9218): a
    // flex-column wrapper holds the Plotly canvas (which flexes) plus the bar, so
    // the canvas shrinks to fit and the bar sits at the bottom inside the box
    // instead of overflowing below it. Scoped to this wrapper so #plot-output's own
    // styles stay untouched and static plots are unaffected (Issue #9242).
    const wrapper = document.createElement('div');
    wrapper.style.cssText = 'display:flex;flex-direction:column;width:100%;height:100%;';
    const plotDiv = document.createElement('div');
    plotDiv.style.cssText = 'width:100%;flex:1 1 auto;min-height:0;';
    wrapper.appendChild(plotDiv);
    if (host) host.appendChild(wrapper);
    const idx = full.map((_, t) => t);
    const sliceTo = (tr, n) => {
        const d = Object.assign({}, tr);
        if (Array.isArray(tr.x)) d.x = tr.x.slice(0, n);
        if (Array.isArray(tr.y)) d.y = tr.y.slice(0, n);
        if (Array.isArray(tr.z)) d.z = tr.z.slice(0, n);
        return d;
    };
    const slicedData = (k) => full.map((tr, t) => sliceTo(tr, counts[k][t]));
    const baseLay = Object.assign({}, layout);
    delete baseLay.updatemenus;
    delete baseLay.sliders;
    delete baseLay._frameDuration;
    const layoutFor = (k) => {
        const l = Object.assign({}, baseLay);
        if (titles) l.title = { text: titles[k], font: { color: '#cdd6f4' } };
        return l;
    };

    // One growing context: extend forward, restyle back. 3D is treated exactly
    // like 2D — start empty and auto-play the growing path so the animation speed,
    // smoothness, and slider behaviour match (Issue #9218).
    let cur = 0;
    let useRestyle = false;
    const restyleTo = (k) => {
        const rx = [], ry = [], rz = [];
        for (let t = 0; t < nTraces; t++) {
            rx.push(full[t].x.slice(0, counts[k][t]));
            ry.push(full[t].y.slice(0, counts[k][t]));
            if (Array.isArray(full[t].z)) rz.push(full[t].z.slice(0, counts[k][t]));
        }
        const rst = { x: rx, y: ry };
        if (rz.length) rst.z = rz;
        Plotly.restyle(plotDiv, rst, idx);
    };
    const showFrame = (k) => {
        if (k === cur) return;
        if (k > cur && !useRestyle) {
            try {
                const ax = [], ay = [], az = [];
                for (let t = 0; t < nTraces; t++) {
                    const a = counts[cur][t], b = counts[k][t];
                    ax.push(full[t].x.slice(a, b));
                    ay.push(full[t].y.slice(a, b));
                    if (has3d) az.push(Array.isArray(full[t].z) ? full[t].z.slice(a, b) : []);
                }
                const ext = { x: ax, y: ay };
                if (has3d) ext.z = az;
                Plotly.extendTraces(plotDiv, ext, idx);
            } catch (e) { useRestyle = true; restyleTo(k); }
        } else { restyleTo(k); }
        cur = k;
        if (titles) Plotly.relayout(plotDiv, { 'title.text': titles[k] });
    };

    await Plotly.newPlot(plotDiv, slicedData(cur), layoutFor(cur), { responsive: true });

    // --- Play/Pause + time-evolution scrubber ---
    const bar = document.createElement('div');
    bar.style.cssText = 'flex:0 0 auto;display:flex;align-items:center;gap:10px;padding:6px 2px 2px;color:#cdd6f4;font:13px sans-serif;';
    const playBtn = document.createElement('button');
    playBtn.textContent = '▶ Play';
    playBtn.style.cssText = 'background:#313244;color:#cdd6f4;border:1px solid #45475a;border-radius:6px;padding:4px 12px;cursor:pointer;';
    const range = document.createElement('input');
    range.type = 'range';
    range.min = '0';
    range.max = String(nFrames - 1);
    range.value = String(cur);
    range.style.flex = '1';
    const lbl = document.createElement('span');
    lbl.textContent = (cur + 1) + '/' + nFrames;
    lbl.style.cssText = 'min-width:64px;text-align:right';
    bar.appendChild(playBtn);
    bar.appendChild(range);
    bar.appendChild(lbl);
    // Bar goes inside the flex wrapper, after the canvas, so it sits at the bottom
    // inside the plot box and the canvas reserves space for it (Issue #9242).
    wrapper.appendChild(bar);
    // The bar just claimed vertical space in the flex column, so the canvas box
    // shrank — resize Plotly so it refits.
    Plotly.Plots.resize(plotDiv);
    const label = () => { range.value = String(cur); lbl.textContent = (cur + 1) + '/' + nFrames; };
    // Draw every frame (stepN = 1) for both 2D and 3D: with the gl3d-leak theory
    // disproved (#9237) the redraw-count cap is unnecessary, so the growing path
    // advances one frame per tick at full temporal granularity (Issue #9241).
    const stepN = 1;
    let timer = null;
    const stop = () => { if (timer) { clearInterval(timer); timer = null; } playBtn.textContent = '▶ Play'; };
    const play = () => {
        if (timer) { stop(); return; }
        if (cur >= nFrames - 1) { showFrame(0); label(); }
        playBtn.textContent = '❚❚ Pause';
        timer = setInterval(() => {
            if (cur >= nFrames - 1) { stop(); return; }
            showFrame(Math.min(cur + stepN, nFrames - 1));
            label();
        }, dur);
    };
    playBtn.addEventListener('click', play);
    // Live "guri-guri" scrubbing for both 2D and 3D: update as the slider is
    // dragged. 3D (gl3d) redraws are heavier, so coalesce rapid drag events with
    // requestAnimationFrame to at most one redraw per frame (Issue #9218).
    let rafPending = false, pendingK = 0;
    range.addEventListener('input', () => {
        stop();
        pendingK = Number(range.value);
        if (has3d) {
            if (!rafPending) {
                rafPending = true;
                requestAnimationFrame(() => { rafPending = false; showFrame(pendingK); label(); });
            }
        } else {
            showFrame(pendingK); label();
        }
    });
    play(); // auto-play the growing path once, for both 2D and 3D
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

// Base warmup: the FIRST run_from_source() call deserializes the embedded
// Base bytecode cache and restores it into the thread-local registries — a
// one-time cost paid on the critical path before the first user execution.
// A trivial `1 + 1` triggers exactly that restore without pulling in the
// (much heavier) Plots package. Run is enabled as soon as THIS completes, so
// users who only `println` no longer wait for the Plots warmup below.
function baseWarmup() {
    if (!wasm) {
        return;
    }
    try {
        wasm.run_from_source('1 + 1\n', BigInt(0));
    } catch (e) {
        console.warn('WASM base warmup failed:', e);
    }
}

// Plot warmup: warms the Plots package compile path so the first user plot is
// fast (Issue #6127). This is heavier than the base warmup, so it runs OFF
// the critical path (via scheduleWarmup, on an idle callback) instead of
// gating Run.
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
        // Only the light Base cache restore is on the critical path. The
        // heavier Plots warmup is scheduled below on an idle callback so it
        // does not delay enabling Run for non-plot usage (e.g. println).
        runBtn.textContent = 'Warming...';
        baseWarmup();
    }

    runBtn.disabled = false;
    runBtn.textContent = runButtonText;

    if (wasm) {
        // Warm the Plots compile path in the background (Issue #6127) without
        // blocking the first user execution.
        scheduleWarmup();
    }
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

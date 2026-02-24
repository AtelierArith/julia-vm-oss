// Julia language definition for Monaco Editor
// Based on Monokai theme colors

// Store wasm module reference for completion provider
let wasmModule = null;

// Built-in functions for code completion (matching iOS app)
const builtinFunctions = [
    // Output
    "println", "print", "display", "show", "repr",
    // Math - Basic
    "abs", "abs2", "sign", "sqrt", "cbrt",
    "exp", "exp2", "exp10", "expm1",
    "log", "log2", "log10", "log1p",
    "sin", "cos", "tan", "asin", "acos", "atan", "atan2",
    "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
    "floor", "ceil", "round", "trunc",
    "min", "max", "clamp", "minmax",
    "mod", "rem", "div", "fld", "cld",
    "gcd", "lcm", "factorial",
    // Math - Advanced
    "hypot", "sincos", "sinpi", "cospi",
    "deg2rad", "rad2deg",
    // Complex
    "complex", "real", "imag", "conj", "angle",
    // Array Creation
    "zeros", "ones", "fill", "rand", "randn",
    "range", "collect", "repeat",
    "trues", "falses",
    // Array Operations
    "length", "size", "ndims", "eltype",
    "push!", "pop!", "append!", "prepend!",
    "insert!", "deleteat!", "empty!",
    "first", "last", "reverse", "sort", "sort!",
    "unique", "union", "intersect", "setdiff",
    // Higher-order Functions
    "map", "filter", "reduce", "foldl", "foldr",
    "foreach", "any", "all", "count",
    "findall", "findfirst", "findlast",
    "sum", "prod", "cumsum", "cumprod",
    "maximum", "minimum", "extrema",
    "argmax", "argmin",
    // Type Functions
    "typeof", "isa", "convert",
    "Int64", "Float64", "String", "Bool",
    // String Functions
    "string", "join", "split", "strip",
    "uppercase", "lowercase", "titlecase",
    "startswith", "endswith", "contains",
    "replace", "match", "occursin",
    "parse", "tryparse",
    // Utility
    "sleep", "time", "error", "throw",
    "isnothing", "something",
    "iszero", "isone", "isnan", "isinf", "isfinite",
    "iseven", "isodd", "ispow2",
    // Iteration
    "enumerate", "zip", "eachindex", "keys", "values",
];

// Keywords for code completion
const completionKeywords = [
    // Control Flow
    "if", "elseif", "else", "end",
    "for", "while", "break", "continue",
    "return", "do",
    // Functions & Types
    "function", "struct", "mutable",
    "abstract", "primitive",
    // Scope & Modules
    "begin", "let", "local", "global", "const",
    "module", "import", "using", "export",
    // Error Handling
    "try", "catch", "finally", "throw",
    // Boolean
    "true", "false", "nothing",
    // Operators
    "in", "isa",
];

// Constants for code completion
const completionConstants = [
    "π", "pi", "ℯ", "Inf", "NaN", "im",
];

// Macros for code completion
const completionMacros = [
    "@time", "@show", "@assert",
];

// Extract variable names from Julia code with usage counts
function extractVariables(code) {
    const variables = new Map(); // name -> count

    const incrementVariable = (varName) => {
        // Filter out keywords and built-in names
        if (completionKeywords.includes(varName) || builtinFunctions.includes(varName)) {
            return;
        }
        variables.set(varName, (variables.get(varName) || 0) + 1);
    };

    // Pattern for variable assignments: name = value
    const assignmentRegex = /(?<![.\w])([a-zA-Z_][a-zA-Z0-9_]*)\s*=/g;
    let match;
    while ((match = assignmentRegex.exec(code)) !== null) {
        incrementVariable(match[1]);
    }

    // Pattern for multiple assignment: x, y, z = values
    const multiAssignRegex = /\(?\s*([a-zA-Z_][a-zA-Z0-9_]*(?:\s*,\s*[a-zA-Z_][a-zA-Z0-9_]*)+)\s*\)?\s*=/g;
    while ((match = multiAssignRegex.exec(code)) !== null) {
        const vars = match[1].split(',');
        for (const v of vars) {
            const name = v.trim().split(':')[0].trim();
            if (name && /^[a-zA-Z_]/.test(name)) {
                incrementVariable(name);
            }
        }
    }

    // Pattern for function parameters: function foo(x, y, z)
    const funcRegex = /function\s+\w+\s*\(([^)]*)\)/g;
    while ((match = funcRegex.exec(code)) !== null) {
        const params = match[1];
        for (const param of params.split(',')) {
            // Handle typed parameters like x::Int
            const name = param.split(':')[0].trim();
            if (name && /^[a-zA-Z_]/.test(name)) {
                incrementVariable(name);
            }
        }
    }

    // Pattern for for loop variables: for i in ... or for (i, j) in ...
    const forRegex = /for\s+\(?\s*([a-zA-Z_][a-zA-Z0-9_,\s]*)\s*\)?\s+in/g;
    while ((match = forRegex.exec(code)) !== null) {
        const vars = match[1].split(',');
        for (const v of vars) {
            const name = v.trim();
            if (name && /^[a-zA-Z_]/.test(name)) {
                incrementVariable(name);
            }
        }
    }

    // Pattern for list comprehension: [expr for x in items] or [expr for x in items, y in others]
    const comprehensionRegex = /\[\s*[^\]]+\s+for\s+([a-zA-Z_][a-zA-Z0-9_]*(?:\s*,\s*[a-zA-Z_][a-zA-Z0-9_]*)*)\s+in/g;
    while ((match = comprehensionRegex.exec(code)) !== null) {
        const vars = match[1].split(',');
        for (const v of vars) {
            const name = v.trim();
            if (name && /^[a-zA-Z_]/.test(name)) {
                incrementVariable(name);
            }
        }
    }

    // Pattern for lambda parameters: (x, y) -> ... or x -> ...
    const lambdaRegex = /\(([^)]+)\)\s*->|([a-zA-Z_][a-zA-Z0-9_]*)\s*->/g;
    while ((match = lambdaRegex.exec(code)) !== null) {
        if (match[1]) {
            // Multiple params: (x, y) -> ...
            for (const param of match[1].split(',')) {
                const name = param.split(':')[0].trim();
                if (name && /^[a-zA-Z_]/.test(name)) {
                    incrementVariable(name);
                }
            }
        } else if (match[2]) {
            // Single param: x -> ...
            incrementVariable(match[2]);
        }
    }

    // Pattern for let blocks: let x = 1, y = 2
    const letRegex = /let\s+([a-zA-Z_][a-zA-Z0-9_]*\s*=\s*[^,\n]+(?:\s*,\s*[a-zA-Z_][a-zA-Z0-9_]*\s*=\s*[^,\n]+)*)/g;
    while ((match = letRegex.exec(code)) !== null) {
        const assignments = match[1].split(',');
        for (const assignment of assignments) {
            const nameMatch = assignment.match(/([a-zA-Z_][a-zA-Z0-9_]*)\s*=/);
            if (nameMatch) {
                incrementVariable(nameMatch[1]);
            }
        }
    }

    // Pattern for catch blocks: catch e or catch ex
    const catchRegex = /catch\s+([a-zA-Z_][a-zA-Z0-9_]*)/g;
    while ((match = catchRegex.exec(code)) !== null) {
        incrementVariable(match[1]);
    }

    // Count variable usage (not just declaration)
    const usageRegex = /\b([a-zA-Z_][a-zA-Z0-9_]*)\b/g;
    while ((match = usageRegex.exec(code)) !== null) {
        const name = match[1];
        if (variables.has(name)) {
            variables.set(name, variables.get(name) + 1);
        }
    }

    return variables;
}

// Extract function names from Julia code with usage counts
function extractFunctions(code) {
    const functions = new Map(); // name -> count

    const incrementFunction = (funcName) => {
        // Filter out keywords and built-in names
        if (completionKeywords.includes(funcName) || builtinFunctions.includes(funcName)) {
            return;
        }
        functions.set(funcName, (functions.get(funcName) || 0) + 1);
    };

    // Pattern for function definitions: function name(...) or function name(...)::Type
    const funcDefRegex = /function\s+([a-zA-Z_][a-zA-Z0-9_!?]*)\s*\(/g;
    let match;
    while ((match = funcDefRegex.exec(code)) !== null) {
        incrementFunction(match[1]);
    }

    // Pattern for short-form function definitions: name(args) = expr
    // Must not be preceded by . (for field access) and not be an assignment to existing variable
    const shortFormRegex = /(?:^|[^.])\s*([a-zA-Z_][a-zA-Z0-9_!?]*)\s*\([^)]*\)\s*(?:::[a-zA-Z_][a-zA-Z0-9_]*)?\s*=/gm;
    while ((match = shortFormRegex.exec(code)) !== null) {
        const funcName = match[1];
        // Exclude common variable patterns
        if (!['let', 'const', 'local', 'global'].includes(funcName)) {
            incrementFunction(funcName);
        }
    }

    // Pattern for lambda assignments: name = (args) -> expr or name = x -> expr
    const lambdaAssignRegex = /([a-zA-Z_][a-zA-Z0-9_!?]*)\s*=\s*(?:\([^)]*\)|[a-zA-Z_][a-zA-Z0-9_]*)\s*->/g;
    while ((match = lambdaAssignRegex.exec(code)) !== null) {
        incrementFunction(match[1]);
    }

    // Count function calls/usage (not just declaration)
    const callRegex = /\b([a-zA-Z_][a-zA-Z0-9_!?]*)\s*\(/g;
    while ((match = callRegex.exec(code)) !== null) {
        const name = match[1];
        if (functions.has(name)) {
            functions.set(name, functions.get(name) + 1);
        }
    }

    return functions;
}

export function setWasmModule(wasm) {
    wasmModule = wasm;
}

export function registerJuliaLanguage(monaco) {
    console.log('[Julia] registerJuliaLanguage called');

    // Register Julia language
    monaco.languages.register({ id: 'julia' });

    // Define tokenizer
    monaco.languages.setMonarchTokensProvider('julia', {
        keywords: [
            'end', 'if', 'elseif', 'else', 'for', 'while',
            'return', 'break', 'continue', 'let', 'in', 'do', 'begin',
            'try', 'catch', 'finally', 'throw',
            'struct', 'mutable', 'abstract', 'primitive', 'type',
            'module', 'baremodule', 'using', 'import', 'export',
            'const', 'global', 'local', 'where', 'isa'
        ],

        builtins: [
            'println', 'print', 'sqrt', 'abs', 'sin', 'cos', 'tan',
            'exp', 'log', 'log10', 'floor', 'ceil', 'round',
            'min', 'max', 'sum', 'prod', 'length', 'size',
            'push!', 'pop!', 'append!', 'sort!', 'reverse!',
            'zeros', 'ones', 'fill', 'rand', 'randn',
            'map', 'filter', 'reduce', 'foreach',
            'typeof', 'convert', 'collect', 'range',
            'ifelse', 'gcd', 'lcm', 'real', 'imag', 'conj',
            'complex', 'adjoint', 'transpose'
        ],

        types: [
            'Int', 'Int8', 'Int16', 'Int32', 'Int64', 'Int128',
            'UInt', 'UInt8', 'UInt16', 'UInt32', 'UInt64', 'UInt128',
            'Float16', 'Float32', 'Float64',
            'Bool', 'Char', 'String',
            'Complex', 'ComplexF64', 'ComplexF32',
            'Array', 'Vector', 'Matrix',
            'Any', 'Number', 'Real', 'Integer', 'AbstractFloat',
            'Nothing', 'Missing', 'Union', 'Tuple', 'NamedTuple', 'Dict',
            'Function', 'Type', 'DataType', 'Symbol'
        ],

        constants: ['true', 'false', 'nothing', 'missing', 'Inf', 'NaN', 'pi', 'π', 'im', 'ℯ'],

        operators: [
            '=', '>', '<', '!', '~', '?', ':',
            '==', '<=', '>=', '!=', '===', '!==',
            '&&', '||', '++', '--',
            '+', '-', '*', '/', '&', '|', '^', '%',
            '<<', '>>', '>>>',
            '+=', '-=', '*=', '/=', '&=', '|=', '^=',
            '%=', '<<=', '>>=', '>>>=',
            '.+', '.-', '.*', './', '.^', '.%',
            '->', '=>', '::', '.', '...'
        ],

        symbols: /[=><!~?:&|+\-*\/\^%]+/,

        escapes: /\\(?:[abfnrtv\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,

        tokenizer: {
            root: [
                // Whitespace
                { include: '@whitespace' },

                // Macros
                [/@[a-zA-Z_]\w*/, 'annotation'],

                // Function definition: function name(
                [/(function)(\s+)([a-zA-Z_]\w*)(\s*)(\()/, ['keyword', 'white', 'function.definition', 'white', 'delimiter.parenthesis']],

                // Short function definition: name(args) = expr
                [/([a-zA-Z_]\w*)(\s*\([^)]*\)\s*=)/, ['function.definition', 'operator']],

                // Function call: name( - but not after 'function' keyword
                [/([a-zA-Z_]\w*!)(\()/, ['function.call', 'delimiter.parenthesis']],  // mutating functions like push!
                // Type constructor call: UpperCase(...)
                [/([A-Z][a-zA-Z0-9_]*)(\()/, {
                    cases: {
                        '$1@types': ['type', 'delimiter.parenthesis'],
                        '@default': ['function.call', 'delimiter.parenthesis']
                    }
                }],
                [/([a-zA-Z_]\w*)(\()/, {
                    cases: {
                        '$1@builtins': ['builtin.function', 'delimiter.parenthesis'],
                        '@default': ['function.call', 'delimiter.parenthesis']
                    }
                }],

                // Type annotation: ::Type
                [/(::)(\s*)([A-Z][a-zA-Z0-9_]*)/, ['operator', 'white', 'type']],

                // Field access: obj.field
                [/(\.)([a-zA-Z_]\w*)/, ['delimiter', 'variable.field']],

                // Keywords (standalone, not followed by parenthesis)
                [/\b(function)\b/, 'keyword'],
                [/[a-zA-Z_]\w*/, {
                    cases: {
                        '@keywords': 'keyword',
                        '@constants': 'constant',
                        '@types': 'type',
                        '@default': 'identifier'
                    }
                }],

                // Delimiters and operators
                [/[{}()\[\]]/, '@brackets'],
                [/@symbols/, {
                    cases: {
                        '@operators': 'operator',
                        '@default': ''
                    }
                }],

                // Numbers
                [/\d*\.\d+([eE][\-+]?\d+)?/, 'number.float'],
                [/0[xX][0-9a-fA-F]+/, 'number.hex'],
                [/0[oO][0-7]+/, 'number.octal'],
                [/0[bB][01]+/, 'number.binary'],
                [/\d+/, 'number'],

                // Delimiter
                [/[;,.]/, 'delimiter'],

                // Strings
                [/"([^"\\]|\\.)*$/, 'string.invalid'],  // non-terminated string
                [/"/, 'string', '@string_double'],

                // Character literals - match complete patterns (before transpose operator)
                [/'[^'\\]'/, 'string'],                  // single char: 'a'
                [/'\\[abfnrtv\\"']'/, 'string'],         // simple escape: '\n'
                [/'\\x[0-9a-fA-F]{2}'/, 'string'],       // hex escape: '\x41'
                [/'\\u[0-9a-fA-F]{4}'/, 'string'],       // unicode escape: '\u0041'

                // Transpose operator (single quote not part of char literal)
                [/'/, 'operator'],

                // Raw strings
                [/raw"/, 'string', '@string_raw'],
            ],

            whitespace: [
                [/[ \t\r\n]+/, 'white'],
                [/#=/, 'comment', '@comment_block'],
                [/#.*$/, 'comment'],
            ],

            comment_block: [
                [/[^#=]+/, 'comment'],
                [/=#/, 'comment', '@pop'],
                [/[#=]/, 'comment']
            ],

            string_double: [
                [/[^\\"$]+/, 'string'],
                [/\$\(/, 'string.interpolation', '@interpolation'],
                [/\$[a-zA-Z_]\w*/, 'string.interpolation'],
                [/@escapes/, 'string.escape'],
                [/\\./, 'string.escape.invalid'],
                [/"/, 'string', '@pop']
            ],

            string_raw: [
                [/[^"]+/, 'string'],
                [/"/, 'string', '@pop']
            ],

            interpolation: [
                [/[^()]+/, 'string.interpolation'],
                [/\(/, 'string.interpolation', '@interpolation_nested'],
                [/\)/, 'string.interpolation', '@pop']
            ],

            interpolation_nested: [
                [/[^()]+/, 'string.interpolation'],
                [/\(/, 'string.interpolation', '@push'],
                [/\)/, 'string.interpolation', '@pop']
            ]
        }
    });

    // Define Monokai-inspired theme for Julia
    monaco.editor.defineTheme('julia-monokai', {
        base: 'vs-dark',
        inherit: true,
        rules: [
            { token: 'keyword', foreground: 'F92672', fontStyle: 'bold' },
            { token: 'function.definition', foreground: 'A6E22E' },  // Green for function definitions
            { token: 'function.call', foreground: '66D9EF' },        // Blue/cyan for function calls
            { token: 'builtin.function', foreground: '66D9EF' },     // Blue/cyan for builtin functions
            { token: 'constant', foreground: 'AE81FF' },
            { token: 'type', foreground: '66D9EF', fontStyle: 'italic' },  // Cyan/italic for types
            { token: 'variable.field', foreground: 'F8F8F2' },       // White for field access
            { token: 'identifier', foreground: 'F8F8F2' },
            { token: 'number', foreground: 'AE81FF' },
            { token: 'number.float', foreground: 'AE81FF' },
            { token: 'number.hex', foreground: 'AE81FF' },
            { token: 'string', foreground: 'E6DB74' },
            { token: 'string.escape', foreground: 'AE81FF' },
            { token: 'string.interpolation', foreground: 'F8F8F2' },
            { token: 'comment', foreground: '75715E', fontStyle: 'italic' },
            { token: 'operator', foreground: 'F92672' },
            { token: 'annotation', foreground: 'A6E22E' },
            { token: 'delimiter', foreground: 'F8F8F2' },
            { token: 'delimiter.parenthesis', foreground: 'F8F8F2' },
            { token: '@brackets', foreground: 'F8F8F2' },
        ],
        colors: {
            'editor.background': '#272822',
            'editor.foreground': '#F8F8F2',
            'editorLineNumber.foreground': '#75715E',
            'editorLineNumber.activeForeground': '#F8F8F2',
            'editor.selectionBackground': '#49483E',
            'editor.lineHighlightBackground': '#3E3D32',
            'editorCursor.foreground': '#F8F8F0',
            'editorWhitespace.foreground': '#3B3A32',
            'editorIndentGuide.background': '#3B3A32',
            'editorIndentGuide.activeBackground': '#767166',
        }
    });

    // Language configuration for brackets, comments, etc.
    monaco.languages.setLanguageConfiguration('julia', {
        comments: {
            lineComment: '#',
            blockComment: ['#=', '=#']
        },
        brackets: [
            ['{', '}'],
            ['[', ']'],
            ['(', ')']
        ],
        autoClosingPairs: [
            { open: '{', close: '}' },
            { open: '[', close: ']' },
            { open: '(', close: ')' },
            { open: '"', close: '"' },
            { open: "'", close: "'" }
        ],
        surroundingPairs: [
            { open: '{', close: '}' },
            { open: '[', close: ']' },
            { open: '(', close: ')' },
            { open: '"', close: '"' },
            { open: "'", close: "'" }
        ],
        indentationRules: {
            increaseIndentPattern: /^\s*(function|if|elseif|else|for|while|try|catch|finally|let|do|begin|struct|mutable|module|baremodule)\b.*$/,
            decreaseIndentPattern: /^\s*(end|else|elseif|catch|finally)\b.*$/
        }
    });

    // Register Unicode completion provider for LaTeX sequences
    monaco.languages.registerCompletionItemProvider('julia', {
        triggerCharacters: ['\\'],
        provideCompletionItems: (model, position) => {
            if (!wasmModule) {
                return { suggestions: [] };
            }

            // Get text before cursor on current line
            const lineContent = model.getLineContent(position.lineNumber);
            const textBeforeCursor = lineContent.substring(0, position.column - 1);

            // Find the last backslash
            const backslashIndex = textBeforeCursor.lastIndexOf('\\');
            if (backslashIndex === -1) {
                return { suggestions: [] };
            }

            // Extract the LaTeX prefix (including backslash)
            const prefix = textBeforeCursor.substring(backslashIndex);

            // Get completions from WASM module
            let completions;
            try {
                completions = wasmModule.unicode_completions(prefix);
            } catch (e) {
                console.error('Unicode completions error:', e);
                return { suggestions: [] };
            }

            if (!completions || !Array.isArray(completions)) {
                return { suggestions: [] };
            }

            // Calculate the range to replace (from backslash to cursor)
            const range = {
                startLineNumber: position.lineNumber,
                startColumn: backslashIndex + 1, // +1 for 1-based indexing
                endLineNumber: position.lineNumber,
                endColumn: position.column
            };

            // Convert to Monaco completion items
            const suggestions = completions.map(([latex, unicode], index) => ({
                label: `${unicode} ${latex}`,
                kind: monaco.languages.CompletionItemKind.Text,
                insertText: unicode,
                range: range,
                detail: latex,
                sortText: String(index).padStart(5, '0'), // Preserve order from WASM
                filterText: latex // Allow filtering by LaTeX command
            }));

            return { suggestions };
        }
    });

    // Register code completion provider for functions, keywords, variables, constants
    monaco.languages.registerCompletionItemProvider('julia', {
        // Trigger on @ for macros (quickSuggestions handles normal typing)
        triggerCharacters: ['@'],
        provideCompletionItems: (model, position) => {
            console.log('[Julia Completion] provideCompletionItems called');

            // Get text before cursor on current line
            const lineContent = model.getLineContent(position.lineNumber);
            const textBeforeCursor = lineContent.substring(0, position.column - 1);

            console.log('[Julia Completion] textBeforeCursor:', textBeforeCursor);

            // Find the current word being typed
            const wordMatch = textBeforeCursor.match(/(@?[a-zA-Z_][a-zA-Z0-9_!?]*)$/);
            if (!wordMatch) {
                console.log('[Julia Completion] no word match');
                return { suggestions: [] };
            }

            const prefix = wordMatch[1];
            const prefixLower = prefix.toLowerCase();

            console.log('[Julia Completion] prefix:', prefix);

            // Don't show completions for very short prefixes (except for @ macros)
            if (prefix.length < 2 && !prefix.startsWith('@')) {
                console.log('[Julia Completion] prefix too short');
                return { suggestions: [] };
            }

            // Calculate the range to replace
            const range = {
                startLineNumber: position.lineNumber,
                startColumn: position.column - prefix.length,
                endLineNumber: position.lineNumber,
                endColumn: position.column
            };

            const suggestions = [];
            let sortIndex = 0;

            // Get the full code for extraction
            const fullCode = model.getValue();

            // Add matching keywords (high priority)
            for (const keyword of completionKeywords) {
                if (keyword.toLowerCase().startsWith(prefixLower)) {
                    suggestions.push({
                        label: keyword,
                        kind: monaco.languages.CompletionItemKind.Keyword,
                        insertText: keyword,
                        range: range,
                        detail: 'keyword',
                        sortText: `0${String(sortIndex++).padStart(3, '0')}` // Keywords first
                    });
                }
            }

            // Add matching built-in functions
            for (const func of builtinFunctions) {
                if (func.toLowerCase().startsWith(prefixLower)) {
                    suggestions.push({
                        label: func,
                        kind: monaco.languages.CompletionItemKind.Function,
                        insertText: func,
                        range: range,
                        detail: 'builtin',
                        sortText: `1${String(sortIndex++).padStart(3, '0')}` // Functions second
                    });
                }
            }

            // Extract and add user-defined functions from the current code
            const userFunctions = extractFunctions(fullCode); // Returns Map<name, count>
            const sortedFunctions = Array.from(userFunctions.entries())
                .sort((a, b) => b[1] - a[1]); // Sort by count, most used first

            for (const [funcName, count] of sortedFunctions) {
                if (funcName.toLowerCase().startsWith(prefixLower)) {
                    suggestions.push({
                        label: funcName,
                        kind: monaco.languages.CompletionItemKind.Function,
                        insertText: funcName,
                        range: range,
                        detail: `user-defined (used ${count}x)`,
                        sortText: `1${String(500 - count).padStart(4, '0')}` // User functions after builtins but before constants
                    });
                }
            }

            // Add matching constants
            for (const constant of completionConstants) {
                if (constant.toLowerCase().startsWith(prefixLower)) {
                    suggestions.push({
                        label: constant,
                        kind: monaco.languages.CompletionItemKind.Constant,
                        insertText: constant,
                        range: range,
                        detail: 'constant',
                        sortText: `2${String(sortIndex++).padStart(3, '0')}` // Constants third
                    });
                }
            }

            // Add matching macros (for @ prefix)
            if (prefix.startsWith('@')) {
                for (const macro of completionMacros) {
                    if (macro.toLowerCase().startsWith(prefixLower)) {
                        suggestions.push({
                            label: macro,
                            kind: monaco.languages.CompletionItemKind.Snippet,
                            insertText: macro,
                            range: range,
                            detail: 'macro',
                            sortText: `0${String(sortIndex++).padStart(3, '0')}` // Macros high priority
                        });
                    }
                }
            }

            // Extract and add variables from the current code
            const variables = extractVariables(fullCode); // Now returns Map<name, count>

            // Convert to array and sort by usage count (descending)
            const sortedVariables = Array.from(variables.entries())
                .sort((a, b) => b[1] - a[1]); // Sort by count, most used first

            for (const [variable, count] of sortedVariables) {
                if (variable.toLowerCase().startsWith(prefixLower) && variable !== prefix) {
                    suggestions.push({
                        label: variable,
                        kind: monaco.languages.CompletionItemKind.Variable,
                        insertText: variable,
                        range: range,
                        detail: `local (used ${count}x)`,
                        sortText: `3${String(1000 - count).padStart(4, '0')}` // Higher usage = lower sort value
                    });
                }
            }

            // Limit results to prevent overwhelming the user (increased from 15 to 30)
            const result = suggestions.slice(0, 30);
            console.log('[Julia Completion] returning', result.length, 'suggestions');
            return { suggestions: result };
        }
    });

    console.log('[Julia] Code completion provider registered');
}

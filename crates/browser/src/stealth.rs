//! Stealth script injection for anti-bot evasion.
//!
//! Injects JavaScript into every page to spoof navigator properties,
//! canvas fingerprints, WebGL info, and other detection vectors.

use cloudyab_types::fingerprint::FingerprintProfile;

/// Generate the full stealth injection script for a given fingerprint profile.
pub fn build_stealth_script(profile: &FingerprintProfile) -> String {
    [
        navigator_spoof(&profile.navigator.user_agent, &profile.navigator),
        webdriver_hide(),
        chrome_runtime_spoof(),
        permissions_spoof(),
        plugins_spoof(),
        webgl_spoof(&profile.webgl.vendor, &profile.webgl.renderer),
        webgl_rendering_noise(),
        canvas_noise_stable(),
        audio_context_spoof(),
        font_enumeration_spoof(&profile.navigator.platform),
        screen_spoof(
            profile.screen.width,
            profile.screen.height,
            profile.screen.color_depth,
            profile.screen.pixel_ratio,
        ),
        window_dimensions_spoof(profile.screen.width, profile.screen.height),
        event_timing_spoof(),
        native_function_masking(),
        webrtc_spoof(),
        cdp_leak_prevention(),
        battery_api_spoof(),
        media_devices_spoof(),
        client_hints_spoof(&profile.navigator),
        notification_permission_spoof(),
        speech_synthesis_spoof(),
        network_info_spoof(),
        document_focus_spoof(),
        intl_locale_spoof(),
        source_url_masking(),
        chrome_app_spoof(),
        image_dimensions_spoof(),
        web_worker_consistency(),
    ]
    .join("\n")
}

fn navigator_spoof(
    user_agent: &str,
    nav: &cloudyab_types::fingerprint::NavigatorProfile,
) -> String {
    format!(
        r#"
Object.defineProperty(navigator, 'userAgent', {{get: () => '{user_agent}'}});
Object.defineProperty(navigator, 'platform', {{get: () => '{platform}'}});
Object.defineProperty(navigator, 'language', {{get: () => '{language}'}});
Object.defineProperty(navigator, 'languages', {{get: () => {languages}}});
Object.defineProperty(navigator, 'hardwareConcurrency', {{get: () => {hw_concurrency}}});
Object.defineProperty(navigator, 'deviceMemory', {{get: () => {device_memory}}});
Object.defineProperty(navigator, 'vendor', {{get: () => '{vendor}'}});
Object.defineProperty(navigator, 'maxTouchPoints', {{get: () => {max_touch}}});
"#,
        user_agent = user_agent,
        platform = nav.platform,
        language = nav.language,
        languages = serde_json::to_string(&nav.languages).unwrap_or_else(|_| "['en-US']".into()),
        hw_concurrency = nav.hardware_concurrency,
        device_memory = nav.device_memory,
        vendor = nav.vendor,
        max_touch = nav.max_touch_points,
    )
}

fn webdriver_hide() -> String {
    r#"
Object.defineProperty(navigator, 'webdriver', {get: () => undefined});
delete navigator.__proto__.webdriver;
"#
    .to_string()
}

fn chrome_runtime_spoof() -> String {
    r#"
window.chrome = {
    runtime: {
        connect: function() {},
        sendMessage: function() {},
        onMessage: {addListener: function() {}, removeListener: function() {}},
        onConnect: {addListener: function() {}, removeListener: function() {}}
    },
    loadTimes: function() { return {}; },
    csi: function() { return {}; }
};
"#
    .to_string()
}

fn permissions_spoof() -> String {
    r#"
const originalQuery = window.navigator.permissions.query;
window.navigator.permissions.query = (parameters) => (
    parameters.name === 'notifications' ?
        Promise.resolve({state: Notification.permission}) :
        originalQuery(parameters)
);
"#
    .to_string()
}

fn plugins_spoof() -> String {
    r#"
Object.defineProperty(navigator, 'plugins', {
    get: () => {
        const plugins = [
            {name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer', description: 'Portable Document Format'},
            {name: 'Chrome PDF Viewer', filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai', description: ''},
            {name: 'Native Client', filename: 'internal-nacl-plugin', description: ''}
        ];
        plugins.length = 3;
        return plugins;
    }
});
"#
    .to_string()
}

fn webgl_spoof(vendor: &str, renderer: &str) -> String {
    format!(
        r#"
const getParameter = WebGLRenderingContext.prototype.getParameter;
WebGLRenderingContext.prototype.getParameter = function(parameter) {{
    const UNMASKED_VENDOR = 0x9245;
    const UNMASKED_RENDERER = 0x9246;
    if (parameter === UNMASKED_VENDOR) return '{vendor}';
    if (parameter === UNMASKED_RENDERER) return '{renderer}';
    return getParameter.call(this, parameter);
}};
const getParameter2 = WebGL2RenderingContext.prototype.getParameter;
WebGL2RenderingContext.prototype.getParameter = function(parameter) {{
    const UNMASKED_VENDOR = 0x9245;
    const UNMASKED_RENDERER = 0x9246;
    if (parameter === UNMASKED_VENDOR) return '{vendor}';
    if (parameter === UNMASKED_RENDERER) return '{renderer}';
    return getParameter2.call(this, parameter);
}};
"#,
        vendor = vendor,
        renderer = renderer,
    )
}

/// Stable canvas noise injection with a per-session seed.
/// The hash stays consistent within a session but differs between profiles.
fn canvas_noise_stable() -> String {
    r#"
(() => {
    const seed = Math.floor(Math.random() * 2147483647);
    function mulberry32(a) {
        return function() {
            a |= 0; a = a + 0x6D2B79F5 | 0;
            let t = Math.imul(a ^ a >>> 15, 1 | a);
            t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
            return ((t ^ t >>> 14) >>> 0) / 4294967296;
        };
    }
    const rng = mulberry32(seed);

    const origToDataURL = HTMLCanvasElement.prototype.toDataURL;
    HTMLCanvasElement.prototype.toDataURL = function(type) {
        const ctx = this.getContext('2d');
        if (ctx && this.width > 0 && this.height > 0) {
            const imageData = ctx.getImageData(0, 0, this.width, this.height);
            const d = imageData.data;
            for (let i = 0; i < d.length; i += 4) {
                d[i] = d[i] ^ (rng() < 0.1 ? 1 : 0);
                d[i+1] = d[i+1] ^ (rng() < 0.1 ? 1 : 0);
                d[i+2] = d[i+2] ^ (rng() < 0.1 ? 1 : 0);
            }
            ctx.putImageData(imageData, 0, 0);
        }
        return origToDataURL.apply(this, arguments);
    };

    const origGetImageData = CanvasRenderingContext2D.prototype.getImageData;
    CanvasRenderingContext2D.prototype.getImageData = function() {
        const imageData = origGetImageData.apply(this, arguments);
        const d = imageData.data;
        for (let i = 0; i < d.length; i += 4) {
            d[i] = d[i] ^ (rng() < 0.1 ? 1 : 0);
            d[i+1] = d[i+1] ^ (rng() < 0.1 ? 1 : 0);
            d[i+2] = d[i+2] ^ (rng() < 0.1 ? 1 : 0);
        }
        return imageData;
    };
})();
"#
    .to_string()
}

/// AudioContext fingerprint spoofing.
/// Injects subtle noise into DynamicsCompressor and AnalyserNode output
/// to produce a unique but stable audio fingerprint per session.
fn audio_context_spoof() -> String {
    r#"
(() => {
    const noiseSeed = Math.random() * 0.0001;

    const origGetFloatFrequencyData = AnalyserNode.prototype.getFloatFrequencyData;
    AnalyserNode.prototype.getFloatFrequencyData = function(array) {
        origGetFloatFrequencyData.call(this, array);
        for (let i = 0; i < array.length; i++) {
            array[i] = array[i] + noiseSeed * (i % 7 - 3);
        }
    };

    const origGetByteFrequencyData = AnalyserNode.prototype.getByteFrequencyData;
    AnalyserNode.prototype.getByteFrequencyData = function(array) {
        origGetByteFrequencyData.call(this, array);
        for (let i = 0; i < array.length; i++) {
            array[i] = Math.max(0, Math.min(255, array[i] + ((i * 7 + 3) % 5 - 2)));
        }
    };

    const origGetChannelData = AudioBuffer.prototype.getChannelData;
    AudioBuffer.prototype.getChannelData = function(channel) {
        const data = origGetChannelData.call(this, channel);
        for (let i = 0; i < data.length; i += 100) {
            data[i] = data[i] + noiseSeed * 0.01;
        }
        return data;
    };

    const origCopyFromChannel = AudioBuffer.prototype.copyFromChannel;
    AudioBuffer.prototype.copyFromChannel = function(dest, channel, start) {
        origCopyFromChannel.call(this, dest, channel, start);
        for (let i = 0; i < dest.length; i += 100) {
            dest[i] = dest[i] + noiseSeed * 0.01;
        }
    };
})();
"#
    .to_string()
}

/// Font enumeration spoofing.
/// Returns a realistic font list matching the claimed OS platform.
fn font_enumeration_spoof(platform: &str) -> String {
    let fonts = if platform.contains("Mac") || platform.contains("mac") {
        r#"[
            "Arial","Courier New","Georgia","Helvetica","Helvetica Neue",
            "Lucida Grande","Monaco","Palatino","Times","Times New Roman",
            "Trebuchet MS","Verdana","American Typewriter","Avenir",
            "Avenir Next","Futura","Geneva","Gill Sans","Menlo",
            "Optima","San Francisco"
        ]"#
    } else {
        r#"[
            "Arial","Arial Black","Calibri","Cambria","Comic Sans MS",
            "Consolas","Courier New","Georgia","Impact","Lucida Console",
            "Microsoft Sans Serif","Palatino Linotype","Segoe UI",
            "Tahoma","Times New Roman","Trebuchet MS","Verdana",
            "Webdings","Wingdings"
        ]"#
    };

    format!(
        r#"
(() => {{
    const fakeFonts = {fonts};
    const defaultWidth = {{}};
    const testString = 'mmmmmmmmmmlli';
    const testSize = '72px';
    const baseFonts = ['monospace', 'sans-serif', 'serif'];
    const span = document.createElement('span');
    span.style.fontSize = testSize;
    span.style.visibility = 'hidden';
    span.style.position = 'absolute';
    span.textContent = testString;

    if (document.fonts && document.fonts.check) {{
        const origCheck = document.fonts.check.bind(document.fonts);
        document.fonts.check = function(font) {{
            const family = font.split(',')[0].replace(/['"]/g, '').trim();
            if (fakeFonts.some(f => f.toLowerCase() === family.toLowerCase())) {{
                return true;
            }}
            return origCheck(font);
        }};
    }}
}})();
"#,
        fonts = fonts,
    )
}

/// Window dimensions consistency with screen values.
/// Ensures innerWidth/innerHeight and outerWidth/outerHeight are consistent.
fn window_dimensions_spoof(width: u32, height: u32) -> String {
    let inner_height = height - 85; // Chrome toolbar + tab bar
    let outer_height = height + 40; // Window chrome
    format!(
        r#"
Object.defineProperty(window, 'innerWidth', {{get: () => {width}}});
Object.defineProperty(window, 'innerHeight', {{get: () => {inner_height}}});
Object.defineProperty(window, 'outerWidth', {{get: () => {width}}});
Object.defineProperty(window, 'outerHeight', {{get: () => {outer_height}}});
Object.defineProperty(document.documentElement, 'clientWidth', {{get: () => {width}}});
Object.defineProperty(document.documentElement, 'clientHeight', {{get: () => {inner_height}}});
"#,
        width = width,
        inner_height = inner_height,
        outer_height = outer_height,
    )
}

/// Reduce performance.now() precision to prevent timing-based fingerprinting.
/// Chrome already does this (100μs precision) but headless may expose higher precision.
fn event_timing_spoof() -> String {
    r#"
(() => {
    const origNow = performance.now.bind(performance);
    performance.now = function() {
        return Math.round(origNow() * 10) / 10;
    };

    const origDateNow = Date.now;
    Date.now = function() {
        return Math.round(origDateNow() / 2) * 2;
    };
})();
"#
    .to_string()
}

/// Mask native function toString to prevent detection of overridden methods.
/// Without this, calling `.toString()` on spoofed functions reveals the override.
fn native_function_masking() -> String {
    r#"
(() => {
    const origToString = Function.prototype.toString;
    const nativePattern = /^function \w+\(\) \{ \[native code\] \}$/;
    const overrides = new Set();

    const handler = {
        apply: function(target, thisArg, args) {
            if (overrides.has(thisArg)) {
                return `function ${thisArg.name || ''}() { [native code] }`;
            }
            return target.call(thisArg);
        }
    };

    Function.prototype.toString = new Proxy(origToString, handler);
    overrides.add(Function.prototype.toString);

    // Mark all our overridden functions
    const propsToMask = [
        navigator.__lookupGetter__('userAgent'),
        navigator.__lookupGetter__('platform'),
        navigator.__lookupGetter__('languages'),
        navigator.__lookupGetter__('hardwareConcurrency'),
        navigator.__lookupGetter__('plugins'),
        navigator.permissions.query,
        HTMLCanvasElement.prototype.toDataURL,
        CanvasRenderingContext2D.prototype.getImageData,
        WebGLRenderingContext.prototype.getParameter,
        performance.now,
    ].filter(Boolean);

    propsToMask.forEach(fn => overrides.add(fn));
})();
"#
    .to_string()
}

fn screen_spoof(width: u32, height: u32, color_depth: u32, pixel_ratio: f64) -> String {
    format!(
        r#"
Object.defineProperty(screen, 'width', {{get: () => {width}}});
Object.defineProperty(screen, 'height', {{get: () => {height}}});
Object.defineProperty(screen, 'availWidth', {{get: () => {width}}});
Object.defineProperty(screen, 'availHeight', {{get: () => {height} - 40}});
Object.defineProperty(screen, 'colorDepth', {{get: () => {color_depth}}});
Object.defineProperty(window, 'devicePixelRatio', {{get: () => {pixel_ratio}}});
"#,
        width = width,
        height = height,
        color_depth = color_depth,
        pixel_ratio = pixel_ratio,
    )
}

/// WebGL rendering hash noise — adds subtle per-session variation to rendered output.
/// Platforms hash the actual rendered 3D scene, not just vendor/renderer strings.
fn webgl_rendering_noise() -> String {
    r#"
(() => {
    const noiseSeed = Math.random() * 0.001;
    const origReadPixels = WebGLRenderingContext.prototype.readPixels;
    WebGLRenderingContext.prototype.readPixels = function() {
        origReadPixels.apply(this, arguments);
        const buf = arguments[6];
        if (buf && buf.length) {
            for (let i = 0; i < buf.length; i += 37) {
                buf[i] = (buf[i] + 1) & 0xFF;
            }
        }
    };
    const origReadPixels2 = WebGL2RenderingContext.prototype.readPixels;
    WebGL2RenderingContext.prototype.readPixels = function() {
        origReadPixels2.apply(this, arguments);
        const buf = arguments[6];
        if (buf && buf.length) {
            for (let i = 0; i < buf.length; i += 37) {
                buf[i] = (buf[i] + 1) & 0xFF;
            }
        }
    };
})();
"#
    .to_string()
}

/// WebRTC IP leak prevention.
/// Replaces real ICE candidates with mDNS hostnames to prevent local IP exposure.
fn webrtc_spoof() -> String {
    r#"
(() => {
    const origCreateOffer = RTCPeerConnection.prototype.createOffer;
    const origCreateAnswer = RTCPeerConnection.prototype.createAnswer;

    function sanitizeSDP(sdp) {
        return sdp.replace(/([0-9]{1,3}\.){3}[0-9]{1,3}/g, '0.0.0.0');
    }

    RTCPeerConnection.prototype.createOffer = function() {
        return origCreateOffer.apply(this, arguments).then(offer => {
            offer.sdp = sanitizeSDP(offer.sdp);
            return offer;
        });
    };

    RTCPeerConnection.prototype.createAnswer = function() {
        return origCreateAnswer.apply(this, arguments).then(answer => {
            answer.sdp = sanitizeSDP(answer.sdp);
            return answer;
        });
    };

    const origAddEventListener = RTCPeerConnection.prototype.addEventListener;
    RTCPeerConnection.prototype.addEventListener = function(type, listener, options) {
        if (type === 'icecandidate') {
            const wrappedListener = function(event) {
                if (event.candidate && event.candidate.candidate) {
                    const sanitized = new RTCIceCandidate({
                        ...event.candidate,
                        candidate: event.candidate.candidate.replace(
                            /([0-9]{1,3}\.){3}[0-9]{1,3}/g, '0.0.0.0'
                        )
                    });
                    listener({...event, candidate: sanitized});
                } else {
                    listener(event);
                }
            };
            return origAddEventListener.call(this, type, wrappedListener, options);
        }
        return origAddEventListener.call(this, type, listener, options);
    };
})();
"#
    .to_string()
}

/// CDP Runtime.enable leak prevention.
/// Prevents detection of CDP presence via Error stack getter trap.
fn cdp_leak_prevention() -> String {
    r#"
(() => {
    // Prevent Error.stack getter trap detection (used by Cloudflare/DataDome)
    const origConsoleDebug = console.debug;
    const origConsoleLog = console.log;

    // Neuter console methods from triggering property access on Error objects
    // by wrapping them to avoid the V8 inspector stack getter trap
    const safeConsole = (orig) => function() {
        const args = Array.from(arguments).map(arg => {
            if (arg instanceof Error) return arg.message || String(arg);
            return arg;
        });
        return orig.apply(this, args);
    };
    console.debug = safeConsole(origConsoleDebug);
    console.log = safeConsole(origConsoleLog);

    // Hide automation-related properties
    delete window.cdc_adoQpoasnfa76pfcZLmcfl_;
    delete document.$cdc_asdjflasutopfhvcZLmcfl_;

    // Remove Puppeteer/Playwright markers
    const automationProps = [
        '__puppeteer_evaluation_script__',
        '__playwright_evaluation_script__',
        '__selenium_unwrapped',
        '_Selenium_IDE_Recorder',
        'callSelenium',
        '_selenium',
        '__webdriver_evaluate',
        '__driver_evaluate',
        '__webdriver_unwrapped',
        '__driver_unwrapped',
        '__fxdriver_evaluate',
        '__fxdriver_unwrapped',
    ];
    automationProps.forEach(prop => {
        try { delete window[prop]; } catch(e) {}
        try { delete document[prop]; } catch(e) {}
    });
})();
"#
    .to_string()
}

/// Battery API spoofing.
/// Returns realistic battery values instead of the default headless response.
fn battery_api_spoof() -> String {
    r#"
(() => {
    if (navigator.getBattery) {
        navigator.getBattery = function() {
            return Promise.resolve({
                charging: true,
                chargingTime: 0,
                dischargingTime: Infinity,
                level: 0.87 + Math.random() * 0.12,
                addEventListener: function() {},
                removeEventListener: function() {},
                dispatchEvent: function() { return true; },
                onchargingchange: null,
                onchargingtimechange: null,
                ondischargingtimechange: null,
                onlevelchange: null,
            });
        };
    }
})();
"#
    .to_string()
}

/// MediaDevices spoofing.
/// Returns realistic device enumeration (headless browsers often return empty).
fn media_devices_spoof() -> String {
    r#"
(() => {
    if (navigator.mediaDevices && navigator.mediaDevices.enumerateDevices) {
        navigator.mediaDevices.enumerateDevices = function() {
            return Promise.resolve([
                {deviceId: '', groupId: 'default', kind: 'audioinput', label: ''},
                {deviceId: '', groupId: 'default', kind: 'videoinput', label: ''},
                {deviceId: '', groupId: 'default', kind: 'audiooutput', label: ''},
            ]);
        };
    }
})();
"#
    .to_string()
}

/// Client Hints (Sec-CH-UA) spoofing.
/// Chrome 120+ uses Client Hints v2 — must match User-Agent claims.
fn client_hints_spoof(nav: &cloudyab_types::fingerprint::NavigatorProfile) -> String {
    let platform = if nav.platform.contains("Mac") {
        "macOS"
    } else {
        "Windows"
    };
    let mobile = if nav.max_touch_points > 0 {
        "true"
    } else {
        "false"
    };

    format!(
        r#"
(() => {{
    Object.defineProperty(navigator, 'userAgentData', {{
        get: () => ({{
            brands: [
                {{brand: "Chromium", version: "125"}},
                {{brand: "Google Chrome", version: "125"}},
                {{brand: "Not.A/Brand", version: "24"}}
            ],
            mobile: {mobile},
            platform: "{platform}",
            getHighEntropyValues: function(hints) {{
                return Promise.resolve({{
                    architecture: "x86",
                    bitness: "64",
                    brands: this.brands,
                    fullVersionList: [
                        {{brand: "Chromium", version: "125.0.6422.112"}},
                        {{brand: "Google Chrome", version: "125.0.6422.112"}},
                        {{brand: "Not.A/Brand", version: "24.0.0.0"}}
                    ],
                    mobile: this.mobile,
                    model: "",
                    platform: this.platform,
                    platformVersion: "{platform_version}",
                    uaFullVersion: "125.0.6422.112",
                    wow64: false,
                }});
            }}
        }})
    }});
}})();
"#,
        mobile = mobile,
        platform = platform,
        platform_version = if platform == "macOS" {
            "14.5.0"
        } else {
            "15.0.0"
        },
    )
}

/// Notification permission state spoofing.
/// Headless browsers return "default" — real browsers vary based on user interaction.
fn notification_permission_spoof() -> String {
    r#"
(() => {
    if (window.Notification) {
        Object.defineProperty(Notification, 'permission', {
            get: () => 'default'
        });
    }
})();
"#
    .to_string()
}

/// SpeechSynthesis voices spoofing.
/// Headless browsers return empty voices list — instant detection signal.
fn speech_synthesis_spoof() -> String {
    r#"
(() => {
    if (window.speechSynthesis) {
        const fakeVoices = [
            {name: 'Microsoft David - English (United States)', lang: 'en-US', default: true, localService: true, voiceURI: 'Microsoft David - English (United States)'},
            {name: 'Microsoft Zira - English (United States)', lang: 'en-US', default: false, localService: true, voiceURI: 'Microsoft Zira - English (United States)'},
            {name: 'Google US English', lang: 'en-US', default: false, localService: false, voiceURI: 'Google US English'},
            {name: 'Google UK English Female', lang: 'en-GB', default: false, localService: false, voiceURI: 'Google UK English Female'},
        ];
        speechSynthesis.getVoices = function() { return fakeVoices; };
        // Fire voiceschanged event
        setTimeout(() => {
            speechSynthesis.dispatchEvent(new Event('voiceschanged'));
        }, 100);
    }
})();
"#
    .to_string()
}

/// Navigator.connection (Network Information API) spoofing.
/// Missing in headless — real Chrome always has this.
fn network_info_spoof() -> String {
    r#"
(() => {
    if (!navigator.connection) {
        Object.defineProperty(navigator, 'connection', {
            get: () => ({
                effectiveType: '4g',
                rtt: 50,
                downlink: 10,
                saveData: false,
                type: 'wifi',
                addEventListener: function() {},
                removeEventListener: function() {},
            })
        });
    }
})();
"#
    .to_string()
}

/// document.hasFocus() spoofing.
/// Always returns false in headless — real browsers return true when tab is active.
fn document_focus_spoof() -> String {
    r#"
(() => {
    document.hasFocus = function() { return true; };
    Object.defineProperty(document, 'hidden', { get: () => false });
    Object.defineProperty(document, 'visibilityState', { get: () => 'visible' });
})();
"#
    .to_string()
}

/// Intl/Locale timezone consistency.
/// Ensures Intl.DateTimeFormat matches the expected timezone for the profile.
fn intl_locale_spoof() -> String {
    r#"
(() => {
    // Ensure Intl reports consistent timezone
    const origResolvedOptions = Intl.DateTimeFormat.prototype.resolvedOptions;
    Intl.DateTimeFormat.prototype.resolvedOptions = function() {
        const opts = origResolvedOptions.call(this);
        // Keep the timezone as-is (should match system/proxy geo)
        return opts;
    };

    // Ensure Date.prototype.getTimezoneOffset is consistent
    const origGetTimezoneOffset = Date.prototype.getTimezoneOffset;
    const cachedOffset = origGetTimezoneOffset.call(new Date());
    Date.prototype.getTimezoneOffset = function() {
        return cachedOffset;
    };
})();
"#
    .to_string()
}

/// Source URL leak prevention in error stacks.
/// When scripts are injected via CDP, error stacks reveal internal URLs
/// like "pptr://" or "__puppeteer_evaluation_script__". This patches
/// Error.prepareStackTrace to sanitize those.
fn source_url_masking() -> String {
    r#"
(() => {
    const origPrepareStackTrace = Error.prepareStackTrace;
    Error.prepareStackTrace = function(error, stack) {
        const filtered = stack.filter(frame => {
            const fileName = frame.getFileName() || '';
            return !fileName.includes('pptr:') &&
                   !fileName.includes('__puppeteer') &&
                   !fileName.includes('__playwright') &&
                   !fileName.includes('__cloudyab') &&
                   !fileName.includes('devtools://');
        });
        if (origPrepareStackTrace) {
            return origPrepareStackTrace(error, filtered);
        }
        return filtered.map(f => `    at ${f}`).join('\n');
    };

    // Also patch Error.stack getter to sanitize existing stacks
    const origStackDesc = Object.getOwnPropertyDescriptor(Error.prototype, 'stack');
    if (origStackDesc && origStackDesc.get) {
        Object.defineProperty(Error.prototype, 'stack', {
            get: function() {
                const stack = origStackDesc.get.call(this);
                if (typeof stack !== 'string') return stack;
                return stack.split('\n').filter(line =>
                    !line.includes('pptr:') &&
                    !line.includes('__puppeteer') &&
                    !line.includes('__playwright') &&
                    !line.includes('__cloudyab') &&
                    !line.includes('devtools://')
                ).join('\n');
            },
            set: origStackDesc.set,
            configurable: true,
        });
    }
})();
"#
    .to_string()
}

/// chrome.app spoofing.
/// Older detection scripts (2020-2023) still check for chrome.app presence.
/// Removed in Chrome 128+ but legacy detectors still look for it.
fn chrome_app_spoof() -> String {
    r#"
(() => {
    if (!window.chrome) window.chrome = {};
    if (!window.chrome.app) {
        window.chrome.app = {
            isInstalled: false,
            InstallState: {DISABLED: 'disabled', INSTALLED: 'installed', NOT_INSTALLED: 'not_installed'},
            RunningState: {CANNOT_RUN: 'cannot_run', READY_TO_RUN: 'ready_to_run', RUNNING: 'running'},
            getDetails: function() { return null; },
            getIsInstalled: function() { return false; },
            installState: function() { return 'not_installed'; },
        };
    }
    if (!window.chrome.csi) {
        window.chrome.csi = function() {
            return {
                onloadT: Date.now(),
                startE: Date.now() - Math.floor(Math.random() * 1000),
                pageT: Math.random() * 3000,
                tran: 15,
            };
        };
    }
    if (!window.chrome.loadTimes) {
        window.chrome.loadTimes = function() {
            return {
                commitLoadTime: Date.now() / 1000,
                connectionInfo: 'h2',
                finishDocumentLoadTime: Date.now() / 1000 + 0.1,
                finishLoadTime: Date.now() / 1000 + 0.2,
                firstPaintAfterLoadTime: 0,
                firstPaintTime: Date.now() / 1000 + 0.05,
                navigationType: 'Other',
                npnNegotiatedProtocol: 'h2',
                requestTime: Date.now() / 1000 - 0.5,
                startLoadTime: Date.now() / 1000 - 0.3,
                wasAlternateProtocolAvailable: false,
                wasFetchedViaSpdy: true,
                wasNpnNegotiated: true,
            };
        };
    }
})();
"#
    .to_string()
}

/// Image dimensions fix for headless browsers.
/// In headless mode without a renderer, Image objects may report 0x0.
/// This ensures naturalWidth/naturalHeight return realistic values.
fn image_dimensions_spoof() -> String {
    r#"
(() => {
    const origImage = window.Image;
    window.Image = function(w, h) {
        const img = new origImage(w, h);
        // Ensure broken images don't report 0x0 (headless detection)
        if (!img.naturalWidth) {
            Object.defineProperty(img, 'naturalWidth', {
                get: () => img.width || 1,
                configurable: true,
            });
            Object.defineProperty(img, 'naturalHeight', {
                get: () => img.height || 1,
                configurable: true,
            });
        }
        return img;
    };
    window.Image.prototype = origImage.prototype;
    Object.defineProperty(window.Image, 'length', { value: 0 });
})();
"#
    .to_string()
}

/// Web Worker navigator consistency.
/// Ensures navigator properties inside Workers match the main thread spoofed values.
fn web_worker_consistency() -> String {
    r#"
(() => {
    // Patch Worker constructor to inject navigator overrides into worker scope
    const origWorker = window.Worker;
    window.Worker = function(url, options) {
        // Workers inherit navigator from the browser — Obscura handles this natively.
        // This patch ensures the Worker constructor itself isn't flagged as modified.
        const worker = new origWorker(url, options);
        return worker;
    };
    window.Worker.prototype = origWorker.prototype;
    Object.defineProperty(window.Worker, 'length', { value: 1 });
})();
"#
    .to_string()
}

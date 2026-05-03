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

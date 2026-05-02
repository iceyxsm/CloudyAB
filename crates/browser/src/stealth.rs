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
        canvas_noise(),
        screen_spoof(
            profile.screen.width,
            profile.screen.height,
            profile.screen.color_depth,
            profile.screen.pixel_ratio,
        ),
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

fn canvas_noise() -> String {
    r#"
const originalToDataURL = HTMLCanvasElement.prototype.toDataURL;
HTMLCanvasElement.prototype.toDataURL = function(type) {
    const ctx = this.getContext('2d');
    if (ctx) {
        const imageData = ctx.getImageData(0, 0, this.width, this.height);
        for (let i = 0; i < imageData.data.length; i += 4) {
            imageData.data[i] ^= 1;
        }
        ctx.putImageData(imageData, 0, 0);
    }
    return originalToDataURL.apply(this, arguments);
};
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

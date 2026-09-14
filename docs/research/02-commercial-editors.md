# 02 — Commercial photo editors other than Photoshop

Research date: 2026-09-14. Scope: every leading commercial editor **except** Adobe Photoshop and Adobe Camera Raw (covered in a separate document). The focus is (a) what these products have that Photoshop does not, (b) their AI features, (c) Lite/simple-mode UX ideas, and (d) pricing and business-model seams that OpenPhotoshop, a local-first Rust/wgpu/wasm editor with Lite and Pro profiles, could use.

## How to read this document

- **Sources.** Every pricing or 2026-status claim comes from a page fetched or searched on 2026-09-14. The Sources section at the end lists them. Some features have been in these products for years (for example, Lightroom Classic's Map and Book modules, or Capture One Sessions). Those are stated without a citation on every line.
- **(unverified).** This marks a claim I could not confirm against a 2026 source. That covers conflicting third-party numbers, details that come only from product knowledge before 2026, and paraphrases where the original page was blocked (HTTP 403).
- **"Not in Photoshop" means not in Photoshop *or* Camera Raw.** Where Photoshop or ACR has a partial equivalent, the table's Note column says so. Those rows should be checked against the Photoshop researcher's document.
- **Quotes.** Quoted complaints are verbatim from the linked page unless marked as a paraphrase.
- **Session limit.** The web-search budget ran out near the end of research. A few late items, such as film-negative conversion and DxO ViewPoint status, could not be re-checked and are marked (unverified).

---

## 0. The 2026 status headlines

| Product | 2026 status in one line |
|---|---|
| **Affinity (Canva)** | Photo, Designer and Publisher became one free app, "Affinity by Canva", in October 2025 (Windows and macOS; iPad "coming soon"). Every Pixel, Vector and Layout tool is free with a free Canva account. The AI tools (Generative Fill, Expand and Edit, background removal, Colorize, Super Resolve, Smart Selections and depth-aware masking, Portrait Blur) need Canva Premium. Version 3.1 shipped on 2026-03-16 and 3.2 on 2026-04-16; 3.2 added Develop masks and a Claude automation connector. |
| **Pixelmator Pro (Apple)** | Apple's acquisition closed in February 2025. On 2026-01-13 Apple announced **Apple Creator Studio** at $12.99/mo or $129/yr, bundling Final Cut Pro, Logic Pro, Pixelmator Pro, Motion, Compressor, MainStage and premium iWork features. Pixelmator Pro reached **iPad on 2026-01-28**, subscription only. The Mac one-time purchase ($49.99) still exists, but the new **Warp tool is subscription-only**. Photomator is still sold separately and is criticised for going without updates. Pixelmator Classic for iOS is end-of-life. |
| **Photopea** | Still free with ads and still one developer (Ivan Kutskir). Revenue was about $3M in 2024, around 90% of it from ads. Premium removes ads and adds 3,000 AI credits a month. Price reports conflict: about $5/mo, or $8 for 30 days, $15 for 90 days, $50 a year (unverified which is current). |
| **Topaz Labs** | **Perpetual licences ended 2025-10-03.** Personal tier: Photo $199/yr, Gigapixel $149/yr, Studio bundle $399/yr. A "Pro" tier for organisations with over $1M in revenue costs $599/$499/$799 a year. There was a strong backlash from former perpetual owners. |
| **Adobe Lightroom** | The Lightroom (1TB) plan rose from $11.99 to **$14.99/mo** (annual, billed monthly) for existing subscribers from 2026-03-20. Classic 15.0 (autumn 2025) added Assisted Culling. 15.3 (April 2026) added natural-language search (cloud only) and "Edit using Describe" on Android. 15.5 (August 2026) added AI mask Feather/Edge, Generative Expand (desktop and iPhone), Render to DNG and "Flatten AI edits" to save credits. |
| **Capture One** | Prices rose **6%** on 2026-06-02, the second 6% rise in consecutive years. Pro is about $18/mo annual, All-in-One about $24.75/mo, Studio about $48.50/mo. 16.7 (2025-10-29) added Combine Masks, Retouch Eyes/Teeth and clothes masking. 16.8 (2026-05-28) added Enhanced Denoise, second-generation Canon wireless tethering, Assisted Review (beta) and team "Actions" that send images to Pixelz, Photoroom or Gemini. |
| **DxO PhotoLab** | **PhotoLab 10 shipped in September 2026** at $249.99 perpetual ($129.99 upgrade from 8 or 9). It adds AI Depth Masks, person and face-part masks, a single-wheel colour grade, AI dust removal, and a direct send to Affinity. DeepPRIME XD3 now covers Bayer and X-Trans sensors (since 9.6). |
| **Luminar Neo (Skylum)** | Perpetual licences cost $129 (desktop), $159 (all platforms) and $199 (Max). Each includes **only one year of AI tools**. After that, **Luminar Prime** ($59/yr) is needed to keep generative tools and the AI Assistant. The Fall 2025 update added Light Depth, Restoration and the AI Assistant. A web version is due in Fall 2026. |
| **ON1 Photo RAW** | 2026 is the current version. **Photo RAW 2027 is announced for fall** with NoNoise AI 2027 (on-device), a Culling Assistant (MAX only), Folder Actions, and a cloud Generative Eraser and Crop for MAX and subscribers. |
| **Corel PaintShop Pro 2026** | One-time purchase, Windows: $79.99, or $99.99 for Ultimate. |
| **Freepik → Magnific** | Freepik renamed itself **Magnific** on 2026-04-28 (ARR $230M). Relight offers three lights plus a reference image. There is a "Precision" upscale mode that avoids hallucination. |
| **Frontier edit models** | **Nano Banana 2** (Gemini 3.1 Flash Image) launched 2026-02-26 with a SynthID watermark. **GPT Image 2 / ChatGPT Images 2.0** launched 2026-04-21/22, and Images 2.5 followed on 2026-09-08 (unverified). Open weights: FLUX.1 Kontext [dev] (12B, June 2025), FLUX.2 (32B, 2025-11-25; "klein" 4B/9B variants) and Qwen-Image-Edit. The large models need about 24 GB of VRAM; klein-class models need 13–16 GB. |
| **Phones** | Google Photos "Help me edit" (Gemini/Nano Banana) is rolled out to US Android and iOS users. Apple Photos in iOS 27 adds Clean Up Fast/High Quality/Auto, Extend and Spatial Reframing (iOS 27 released 2026-09-14 per MacRumors). Samsung Galaxy S26 Photo Assist takes text prompts and keeps a reviewable edit history. Snapseed 3.0 has been on iOS since June 2025, with Android due in 2026. |

---

## 1. Per-product sections

### 1.1 Adobe Lightroom Classic and Lightroom (cloud-based)

**Pricing (2026).**
- Lightroom plan (1TB): **$14.99/mo** annual billed monthly, $149.99/yr prepaid, $22.49/mo month-to-month (existing subscribers from 2026-03-20).
- Photography plan (20GB): $14.99/mo for new customers since the 2025 rise (TechRadar).
- Photography plan (1TB): about $19.99/mo annual (unverified; Imagen blog).
- Generative AI features spend **generative credits**. Forum members cite 500 credits a month included (unverified exact figure).

**Platform.** Classic runs on Windows and macOS. Lightroom (cloud-based) runs on Windows, macOS, iOS, iPadOS, Android and the web.

**Business model.** Subscription only, with metered generative credits on top.

**Headline differentiators versus Photoshop.**
- A full DAM: catalog, collections, smart collections, keywords, People, Map, publish services.
- Culling and tethering (Canon, Nikon, Sony, Fujifilm and, since Classic 15, Leica).
- Books, Slideshow (4K since Classic 15), Print and Web modules.
- Presets and profiles, virtual copies, Smart Previews.
- Cross-device sync (cloud-based Lightroom).
- Mobile Quick Actions and adaptive presets.

**AI features (2025–2026).**

| Feature | Release | On-device or cloud / credits |
|---|---|---|
| Assisted Culling (subject focus, eye focus, exposure-issue sliders, sensitivity) | Classic 15; improved in 15.3 | The Lightroom Queen says 15.3 culling "requires generative AI credits" |
| Auto Stack by visual similarity (also millisecond-precision time stacking) | Classic 15 | Local (PhotoshopCAFE) |
| Automatic dust-spot removal, reflection removal, shadow detection in Remove | Classic 15 | Dust is local (unverified); generative Remove is Firefly cloud |
| Variance in Point Color, Snow landscape mask | Classic 15 | Local (shared with ACR) |
| Natural-language search ("raw photos of a white dog and a child") | 15.3 / desktop 9.3 | **Cloud mode only** |
| Edit using Describe (styles Natural, Vintage, Cinematic, Light & Airy, Warm & Earthy; or free text such as "make this warmer") | Android mobile, April 2026 | **Credits** |
| Firefly Mood Boards | 15.3 | Firefly credits |
| Background AI processing for copy, paste and sync | 15.3 | Local |
| AI mask Feather and Edge sliders | 15.5 | Local |
| Generative Expand | Desktop 9.5 and iPhone | **Credits** |
| Animate (still photo to short video) | iOS and Android 11.5 | **Credits** (cost shown beneath the button) |
| Flatten AI Edits (bake earlier AI edits to avoid re-spending credits) | 15.5 | — |
| Denoise on iPad (M1 or later) | Mobile 11.5 | Local |
| Quick Actions, adaptive presets, recommended presets | Mobile, desktop, web | Adaptive is local; recommended presets content is cloud (unverified) |

**Notable weaknesses.**
- AI features are split into credit-metered and free tiers in a way users find hard to predict.
- Some features are cloud-mode only.
- Reported 15.5 bugs include Windows video issues, import failures, GPS badge errors, export skipping with Content Credentials, and Android crop snapping.
- Users regularly report "Generative Remove Failed" errors.

**User complaints.**
- "looks like the remove tool will become a paying feature … curious if its per image or by stroke or something" — user *hei*, 2026-03-08, [Lightroom Queen forum](https://www.lightroomqueen.com/community/threads/removal-tool-generative-becoming-a-paying-feature.54470/). Other members replied that Classic's Remove did not consume credits at the time. The anxiety about metering is the signal.
- Repeated "Generative Remove Failed" threads, including from users who still had credits left: [Lightroom Queen, "Generative Removed Failed"](https://www.lightroomqueen.com/community/threads/generative-removed-failed.54089/) and ["Bad (unusable) AI results and credits"](https://www.lightroomqueen.com/community/threads/bad-unusable-ai-results-and-credits.53553/).

### 1.2 Capture One Pro, All-in-One and Studio

**Pricing (after the 6% rise on 2026-06-02).**

| Tier | Annual (per month) | Rolling monthly |
|---|---|---|
| Pro | ~$18 | — |
| All-in-One (desktop, iPad, iPhone) | ~$24.75 | ~$38 |
| Studio | ~$48.50 | ~$62 |

Perpetual licences still exist and also rose 6%; the exact figure is unverified. This is the second consecutive 6% rise (March 2025, then June 2026).

**Platform.** Windows and macOS. There is also Capture One for iPad and iPhone.

**Business model.** Subscription or perpetual, with a Studio/Teams/Enterprise ladder. Commentators link the price rises to private-equity ownership (paraphrase from a DPReview thread; page 403, unverified verbatim).

**Headline differentiators.**
- Colour science and the Color Editor with skin-tone uniformity.
- The best tethering in the category: Live View, Next Capture Adjustments, Overlay, ReTether, second-generation Canon wireless tethering.
- **Sessions**: portable per-job folder projects, alongside catalogs.
- Layer-based raw editing with per-layer opacity.
- Stackable **Styles** and **Speed Edit** (hotkey plus scroll to change any value).
- Multiple simultaneous **Process Recipes** on export.
- Smart Adjustments and Match Look.
- Capture One Live for remote client viewing.

**AI features.**
- **AI Masking**: subject, background, people with face, skin, hair and clothes (clothes added in 16.7), Combine Masks.
- **Smart Adjustments**: matches exposure and white balance to a reference across a set.
- **Match Look**: applies the grade of any dropped-in image.
- **Retouch**: Blemish Removal, Even Skin, Include Neck Area, Retouch Eyes, Retouch Teeth (16.7).
- **Enhanced Denoise** (16.8): Bayer raw only. Not X-Trans, JPEG, TIFF, PSD, sRAW/mRAW or ProRAW. Runs in the background.
- **Assisted Review (beta, 16.8)**: flags closed eyes, missed focus, exposure problems and black frames. Capture One positions it as a filter, not auto-selection.
- AI auto-rotate for batches.
- Snapdragon NPU acceleration (16.7.2), which implies on-device inference for "select AI tools". Where each AI feature runs is not fully documented (unverified).
- **Actions** (Studio for Teams and Enterprise): sends images to Pixelz, Photoroom or Gemini, all cloud.

**Notable weaknesses.**
- Price escalation.
- Native AI masking is seen as behind Lightroom.
- Enhanced Denoise's format limits.
- The "upgrade only for new camera support" feeling reported by long-time users.

**User and reviewer complaints.**
- Price rises are "starting to feel less like a pricing adjustment and more like a habit" — reviewer analysis, [Newsshooter, 2026-06-01](https://www.newsshooter.com/2026/06/01/capture-one-raises-prices-again-is-it-still-worth-it-or-time-to-jump-ship/) (wording as returned by search; the fetched page confirmed the tone).
- A forum user says Capture One is "cashing in on their existing, studio tethered focused user base since being bought by private equity" — paraphrase from a search summary of the [DPReview thread "Capture one price increase from 2 June"](https://www.dpreview.com/forums/threads/capture-one-price-increase-from-2-june.4836714/) (page returned 403; unverified verbatim).

### 1.3 Affinity (Canva), formerly Affinity Photo 2

**Pricing.**
- **Free.** Every tool in the Pixel, Vector and Layout studios, plus all export features, needs only a free Canva account.
- **AI tools need a Canva paid plan**: Generative Fill, Expand and Edit; Image and Vector generation; background removal; Colorize; portrait enhancement; Smart Selections and depth-aware masking; Portrait Blur; Super Resolve.
- Canva Pro costs $15/mo or $120/yr (eesel, Style Factory). One photography blog cites $144–$250/yr for paid tiers (unverified; likely regional or Teams pricing).

**Platform.** Windows and macOS with identical apps. iPad is "coming soon". Activation needs an internet connection; after that the app works offline, but AI, stock and help need connectivity.

**Business model.** Free professional software funded by upsell to Canva's AI and Premium plans. Canva's public reasoning is in "Why we made Affinity free, and how we'll keep it that way". More than 1M sign-ups arrived in the first four days (TechRadar via search).

**Headline differentiators (all free).**
- Unified Studios: Pixel, Vector, Layout, Slice, Retouching, Colour Grading, Compositing. These replace the old personas.
- The Develop, Liquify and Tone Map workspaces now live under Pixel > Filter.
- **Live filters** as layers.
- **HDR Merge**, **Focus Merge**, **Astrophotography Stacking** with calibration frames.
- **Macros** and batch jobs.
- A 32-bit workflow.
- **Frequency Separation** as a built-in filter.
- **Procedural Texture**: an equation-driven filter.
- Inpainting, mesh warp, and PSD import "with layer fidelity".
- New in 3.x:
  - Bloom live filter; Adjustment and Filter brushes (3.0).
  - Light UI; **Tone Brush**; **Live Tone Blend Groups** (3.1).
  - Develop masks: Object Selection, Luminosity, Hue Range, Compound (3.2).
  - Texture filter, Multi Band Sharpen with Fine Detail, focus peaking in Develop (3.2).
  - DaVinci Resolve live link and Brand Kits (3.2).
  - A **Claude connector** that turns plain-language descriptions into reusable automation scripts (3.2).

**AI features.**
- All generative and "smart" features run through Canva's cloud AI and need Premium.
- Affinity 3.2's Develop "Object Selection" is described as AI-assisted. Whether it is free and on-device is unverified; Affinity's own page lists "Smart Selections" as Premium.
- The Claude automation connector is cloud-based.

**Notable weaknesses.**
- A new file format that is not backward compatible with Affinity Photo 2.
- UI relearning; some plugin incompatibility (Nik Collection).
- Crash and shortcut-conflict reports.
- No DAM or catalog.
- "Little automation beyond basic presets" for large sets (flypix review, via search).
- A **Canva account is mandatory**.

**User and reviewer complaints.**
- "As long as I don't need to login to a Canva account to use Affinity, I'll be fine. If they start subscriptions to Canva as a requirement for the Affinity software to work, I'm out." — [Threads, @douglaspmarx](https://www.threads.com/@douglaspmarx/post/DQe4c-tEfEB/as-long-as-i-dont-need-to-login-to-a-canva-account-to-use-affinity-ill-be-fine-i)
- Users called the new interface "confusing," "cluttered," or "overwhelming" — [Lenscraft, "Affinity 3 for Photographers"](https://lenscraft.co.uk/photography-blog/affinity-3-for-photographers/). On the Claude automation: "not a production-ready tool yet" — [Beyond Photo Tips](https://www.beyondphototips.com/whats-new-affinity-3-2-for-photographers/).

### 1.4 Pixelmator Pro and Photomator (Apple)

**Pricing.**
- **Apple Creator Studio**: $12.99/mo or $129/yr; education $2.99/mo or $29.99/yr; one-month trial; three months free with a new Mac or qualifying iPad.
- **Pixelmator Pro for Mac, one-time**: $49.99. It **does not get the Warp tool**, and "certain AI features aren't included in the one-time-purchase version" (MacRumors via search; which features is unverified).
- **iPad**: available only through Creator Studio (Apple's page lists the one-time purchase as "Mac only").
- **Photomator**: not in the bundle. Separate subscription (about $30–$40/yr) or lifetime (about $119, unverified 2026).

**Platform.**
- Pixelmator Pro: macOS 26+ and iPadOS 26+ (M1, A16 or A17 Pro iPads).
- Photomator: Mac, iPhone, iPad, Vision Pro.

**Business model.** Apple's services bundle, with subscriber-exclusive features added over time and a Content Hub of premium templates and graphics.

**Headline differentiators.**
- A native Apple UI (Liquid Glass on iPad).
- Nearly identical Mac and iPad apps with iCloud sync.
- Warp with 12 smart presets (cylinder, arc, flag…).
- Vector shapes and typography, templates and mockups, video layers.
- Over 750 RAW formats.
- Photomator: Photos-library-native browsing, full batch editing (presets, enhance, crop, rotate, denoise, watermark), Smart Deband.

**AI features.**
- ML Super Resolution, ML Enhance (trained on 20M professional photos), Denoise, Deband / Smart Deband.
- Select Subject, Sky and Background; Remove Background.
- ML Crop and Auto Straighten; auto white balance; Match Colors; Repair (object removal).
- **On-device.** Pixelmator has documented Core ML models "integrated in the app package" since 2019. Apple's 2026 page says AI "builds on Apple Intelligence" without naming where it runs, so 2026 location is unverified.

**Notable weaknesses.**
- Apple platforms only.
- Specialised effects don't survive PSD export.
- No animation.
- Unreliable auto layer selection.
- The subscription/one-time split is confusing.
- Photomator neglected since the acquisition.

**Complaints.**
- "if there's a subscription fee I'm paying annually, then there needs to be at least annual development of the software." — [Six Colors](https://sixcolors.com/post/2026/01/whats-the-real-value-in-creator-studio/), on Photomator and Creator Studio. Also: the split between subscription and free features "makes very little sense, and will probably make even less sense over time."
- Cons: "Specialized effects don't transfer when saving as .psd files"; "Auto layer selection can be unreliable" — [Digital Camera World, Pixelmator Pro 4 review](https://www.digitalcameraworld.com/tech/software/pixelmator-pro-4-review) (as summarised).

### 1.5 Photopea

**Pricing.**
- Free with ads, with every editing feature available.
- Premium removes ads, raises storage from 0.5 GB to 5 GB, adds more history steps and **3,000 AI credits a month**. Reports disagree on price: about $5/mo (Shotkit, Built Plain) or $8 for 30 days, $15 for 90 days, $50/yr (checkthat.ai) (unverified).
- Self-hosting licence: $500–$2,000/mo, prepaid yearly.

**Platform.** Any modern browser (Chrome, Firefox, Safari, Edge, Opera). Client-side JavaScript; files stay on the user's machine unless saved to PeaDrive.

**Business model.**
- Solo developer (Ivan Kutskir, Prague).
- About $3M revenue in 2024, **90% from ads** and about 10% from Premium and licensing.
- About $12,600 a year in costs, $12,000 of it AI inference.
- About 1M daily active users and 1.5M user-hours a month in the editor (Built Plain).

**Headline differentiators.**
- The closest Photoshop clone in UI and shortcuts.
- Opens PSD (smart objects, adjustment layers, layer styles), XCF, Sketch, XD, PDF, SVG and RAW/DNG.
- Zero install; embeddable API.
- PSD fidelity failures are "confined to exotic territory": a few rarely used layer effects render slightly differently, and one unusual blend mode is approximated (search summary of a 2026 review).

**AI features (all cloud).**
- **Magic Replace** (May 2023): a selection plus an optional prompt; generated content is blended in, with a choice of AI models.
- Background removal and image generation (Stable Diffusion-class models).
- Free users get AI **once a day** (Wikipedia) or by watching ads (other reviews); premium users spend credits.

**What it lacks.**
- Camera Raw parity; generative fill quality; some smart-object behaviours.
- Performance on large files, since it is browser- and RAM-bound.
- Offline-first workflows.
- No DAM, culling or tethering.

**Complaints.**
- Cons: "Ad blockers cause compatibility issues" and "Slower browser-based performance" — [Shotkit review](https://shotkit.com/photopea-review/) (as summarised).
- "less suited for offline-first workflows" — search summary of [digi-tools.info review](https://digi-tools.info/articles/photopea-review) (unverified verbatim).

### 1.6 Luminar Neo (Skylum)

**Pricing (September 2026 page).**

| Licence | Price | Includes |
|---|---|---|
| Desktop perpetual | $129 | Desktop lifetime, **1 year of AI tools** |
| All Platforms perpetual | $159 | Desktop, mobile and web (web due Fall 2026), 1 year of AI |
| Max perpetual | $199 | All of the above plus 1 year of **Luminar Prime** (renews at **$59/yr**) |

- Luminar Prime restores generative access (GenErase, GenSwap, GenExpand, Restoration, AI Assistant) and the asset library.
- A fair-use policy limits bulk generation.
- Luminar X Elite membership: $39/yr.

**Platform.** Windows and macOS, plus mobile. Plugins for Photoshop and Lightroom.

**Business model.** "Perpetual" licences whose AI access expires, plus heavy discounting (list prices are routinely 40–50% off).

**Headline differentiators.**
- One-slider AI tools: Enhance AI, Sky AI, Structure AI, Atmosphere AI.
- **Light Depth**: 3D relighting that replaced Relight AI in Fall 2025; free for all users.
- Portrait tools: Portrait Bokeh, Face AI, Skin AI, Body AI.
- Sunrays, Glow, Mood (LUTs).
- Supersharp AI, Noiseless AI, Upscale AI.
- HDR Merge, Panorama Stitching, Focus Stacking.
- **Restoration**: repairs cracks, stains and fading, and colorises.
- **AI Assistant**: continuous analysis with tips and next-step suggestions, not a prompt editor.
- Spaces web galleries, cross-device Ecosystem, Photoshop and Lightroom plugins.

**AI features.**
- Most tools run on-device.
- The generative tools (GenErase, GenSwap, GenExpand) are cloud-based (unverified exact split).
- The AI Assistant and Restoration are gated by pass or licence tier.

**Weaknesses.**
- Catalog and DAM weaker than Lightroom.
- Batch slowness on large jobs; Windows performance complaints.
- Plugin versions lack features of the main app.
- Confusing pass and ecosystem pricing.

**Complaints.**
- "One star off for not generally communicating that the Luminar plug-in app in Photoshop and Lightroom would be missing some key elements of the upgrade." — Tom (CH), 2026-09-06, [Trustpilot](https://www.trustpilot.com/review/skylum.com)
- "the former Relight feature, which in my view didn't work very well" — [Fstoppers Fall Update review](https://fstoppers.com/artificial-intelligence/review-luminar-neo-fall-update-winner-some-powerful-new-tools-715721). The same reviewer called Light Depth "worth the upgrade".

### 1.7 DxO PhotoLab 10 (plus PureRAW, FilmPack, ViewPoint, Nik Collection)

**Pricing.**
- PhotoLab 10 Elite: **$249.99** lifetime; upgrade from 8 or 9 is $129.99.
- Essential edition exists; price unverified.
- Elite activates on 3 computers, Essential on 2.
- Needs an online check every 37 days.

**Platform.** Windows 10/11 and macOS 15.7+; 16 GB RAM; RTX 3000-class GPU recommended for AI.

**Business model.** Perpetual licences with paid major upgrades, cross-sold with FilmPack, ViewPoint, PureRAW and Nik.

**Headline differentiators.**
- **DeepPRIME XD3**: joint AI demosaicing and denoising, now for Bayer and X-Trans.
- Lab-measured **optics modules** for camera and lens pairs, including lens softness.
- **U-Point control points.**
- **AI Depth Masks** (PL10); AI masks for individual people and face parts (eyebrows, irises and pupils, sclera, teeth, lips, facial hair, skin); **Control Brush** (brush plus eyedropper).
- AI dust removal (from PureRAW 6); single-wheel colour grading; AI mask diffusion (9.6).
- **High-Fidelity Compression** DNGs up to 4× smaller (9.6).
- One-click send to Affinity.

**AI features.** All on-device: DeepPRIME, AI masks, depth masks and dust removal run locally on GPU.

**Weaknesses.**
- Library tools are basic and folder-based.
- Few creative presets.
- DeepPRIME is previewed only in a loupe window, not live on the whole image.
- Expensive.

**Complaint.** PhotoLab lacks "the powerful searching and organising tools" of Lightroom and Capture One — [Life after Photoshop, PhotoLab 10 review](https://lifeafterphotoshop.com/dxo-photolab-10-review/).

### 1.8 ON1 Photo RAW 2026 (and the announced 2027)

**Pricing.**
- Perpetual about $99.99 (regular) and MAX about $169.99; subscription about $79.99/yr; upgrades about $79.99. All unverified; these figures come from reviews and sale pages.
- A 2027 pre-order includes 2026 and 2027.

**Platform.** Windows and macOS, plus ON1 mobile.

**Business model.** Perpetual or subscription. MAX and subscriber tiers get cloud generative tools and the new Culling Assistant.

**Headline differentiators.**
- A hybrid browser: work from folders or catalog.
- Layers inside the raw editor; Effects stacks with per-filter masks.
- NoNoise AI and Resize AI; Brilliance AI.
- **Cinematic Depth Lighting** (AI depth masks), Super Select AI, Sky Swap, Portrait AI.
- HDR, panorama and focus stacking.
- 2027 plans: Culling Assistant (similar groups, closed eyes, out-of-focus shots), Tack Sharp deblur, genre-specific NoNoise models (wildlife, astro, portrait, macro), **Folder Actions** (watch folders apply metadata, presets and exports), Loupe and Compare views, Windows ML execution providers, a simpler unified masking UX.

**AI features.**
- On-device: NoNoise AI 2027, Culling Assistant, Brilliance AI.
- Cloud: Generative Eraser and Crop (MAX and subscribers).

**Weaknesses.** Performance and stability. Users say 2026 was much slower than 2025, with export crashes from memory leaks in 2026.2, first-use AI model downloads, and slow Quickmask AI and SuperSelect AI.

**Complaints.**
- 2026 was "impossibly slower than 2025"; exporting a couple of hundred images crashed ON1 on Windows and the whole system on Mac — paraphrase from search summaries of [DPReview "ON1 Raw 2026 review"](https://www.dpreview.com/forums/threads/on1-raw-2026-review.4823008/) and [Cameraderie](https://cameraderie.org/threads/on1-photo-raw-2026.59815/) (unverified verbatim).

### 1.9 Topaz Photo and Gigapixel (Topaz Labs)

**Pricing (official page, September 2026).**

| Product | Personal annual | Personal monthly | Pro annual (orgs over $1M revenue) |
|---|---|---|---|
| Topaz Photo | $199 | $39 | $599 |
| Topaz Gigapixel | $149 | $29 | $499 |
| Topaz Video | $299 | $59 | $699 |
| Topaz Studio (all 8 apps) | $399 (or $45/mo annual commitment, $69/mo) | — | $799 |
| Topaz for Web – Image | $12/mo ($109/yr) | — | $35/mo |

- Local rendering is unlimited.
- Studio includes unlimited cloud image rendering and 300 video credits (600 on Pro).
- Pro raises export limits (100 MP versus 32 MP) and concurrency.

**Business model.** **Subscription only since 2025-10-03.** Existing perpetual owners keep their versions but get no new models.

**Headline differentiators.**
- Topaz Photo bundles **11 model families**: Wonder 2 and 3, Standard, Standard Max, High Fidelity, Low Res, Text & Shapes, Art & CG, Recover, Redefine (generative), Face Recovery.
- Other tools: Super Focus deblur, dust and scratch removal, remove, grain, lighting and colour balance.
- **Autopilot** detects subjects and faces and *suggests* settings without applying them.
- **NeuroStream** (March 2026) cuts VRAM needs by up to 95%.
- Genre optimisations (April 2026).
- Plugins for Lightroom Classic, Photoshop, Capture One and Apple Photos.

**AI location.** Local by default; some "Max" and Redefine models can render in the cloud.

**Weaknesses.**
- Value after the subscription switch.
- Slow and crash-prone on large upscales and older machines.
- Cloud rendering blocks the UI.
- Inconsistent model quality (for example, Standard beating the cloud Standard Max).

**Complaints.**
- "What once cost $199 outright now costs $199 every year" and "the cloud rendering service has taken a step back…the application locks up" — [Silent Peak Photo review](https://silentpeakphoto.com/photo-editing-apps/photo-editing-app-reviews/topaz-photo-ai-review/)
- "I paid ninety-nine dollars for a tool that worked. Now they want one hundred and ninety-nine dollars every year for the same thing." — Reddit sentiment as quoted by [Sammapix](https://www.sammapix.com/blog/best-free-topaz-gigapixel-alternatives-2026) (original thread unverified). The open-source Upscayl (Real-ESRGAN) is the most-cited free alternative ([AlternativeTo](https://alternativeto.net/software/a-i-gigapixel)).

### 1.10 Corel PaintShop Pro 2026

- **Pricing:** $79.99, upgrades about $60; Ultimate $99.99, bundled with Painter Essentials, PhotoMirage Express, Perfectly Clear SE and AfterShot. One-time purchase, no subscription.
- **Platform:** Windows only.
- **Differentiators:** Art Media brushes (acrylic, oil, watercolour), Pic-to-Painting, and Python scripting (unverified 2026).
- **AI features:** AI Upsampling, Denoise, Artifact Removal, Portrait Mode, Background Replacement, Style Transfer. On-device (unverified).
- **Weaknesses:** Windows only; AI quality behind leaders (unverified). I found no verified 2026 user quotes.

### 1.11 Serif (now Canva)

Serif remains the development studio inside Canva. There is no longer a separate Serif consumer product line; see 1.3. The earlier joint Canva/Serif "Pledge" promised no mandatory subscriptions and continued perpetual licences for existing products (per AlternativeTo and TechRadar via search).

### 1.12 Canva photo editor (Magic Studio)

**Pricing.**
- Free: one monthly pool of up to 200 Standard or 20 Premium AI uses.
- Pro: $15/mo or $120/yr, with 2,000 Standard or 200 Premium uses, background remover and Magic Resize.
- Background Remover, Magic Eraser and Magic Expand run outside the credit pool on "fair use" (fast.io).

**AI features (cloud).**
- Magic Edit (prompt replace); Magic Eraser (brush or click).
- **Magic Grab** (detected object becomes draggable; hole auto-filled).
- Magic Expand, Background Remover, Magic Layers, Dream Lab (generation).
- **Canva AI 2.0**, a conversational layer that chains tools in one request and uses up the allowance fastest.

**Reviewer verdicts.**
- Magic Eraser is "one of the most consistently reliable tools in the suite".
- Magic Edit is "a useful tool for quick fixes, not a Photoshop replacement".
- Magic Grab produces "messier extractions" in busy scenes.
- Magic Expand "generates visible artifacts at the seam".
- Magic Design tends toward "design homogeneity".
- Source: [fast.io](https://fast.io/resources/canva-ai-review-2026/).

### 1.13 Picsart

- **Pricing (2026 restructure):** Free, 5 credits a week, no top-ups. Plus. Pro at $10.50/mo billed yearly ($15 monthly), with 500 credits a month, 100 GB, brand kits, bulk edit of up to 50 images, "15+ creative AI agents" and a CLI (Flowith blog; unverified).
- **AI features (cloud):** background removal, AI enhance, AI replace, sticker maker, text-to-image, model hub (for example, GPT Image 2).
- **Complaint:** on the free plan "AI features are heavily restricted and you cannot purchase additional credits unless you upgrade" ([Flowith](https://flowith.io/blog/picsart-pricing-free-vs-plus-vs-pro-plan/), reviewer summary).

### 1.14 Snapseed (Google)

- **Pricing:** free, no ads (long-standing).
- **Status:** 3.0 redesign on iOS in June 2025, the first major update since 2021. The Android redesign was confirmed in January 2026.
- **UX:**
  - Home is a grid of past edits with a "+" floating action button.
  - The editor has **Looks / Faves / Tools** tabs.
  - Tools fall into four groups: Adjust & Correct; Retouch & Transform (includes Expand and Head Pose); Style (new Film, Glow, Retrolux, Vintage, B&W, HDR Scape, Drama, Noir, Grunge); Creative (Double Exposure, Frames, Text).
  - A new arc slider responds to horizontal drags; some tools still switch parameter with a vertical swipe.
  - Export moved to the top right.
  - A camera mode applies Looks and film simulations live.

### 1.15 Apple Photos (iOS 26 / iOS 27, macOS)

- **Pricing:** free with the device.
- **Tabs:** Adjust, Filters, Crop, Clean Up.
- **Controls:**
  - Auto wand ("on-device smarts").
  - On Mac, **Light, Color and Black & White master sliders**, each expanding into sub-sliders. Light covers Brilliance, Exposure, Highlights, Shadows, Brightness, Contrast and Black Point. Color covers Saturation, Vibrance and Cast. B&W covers Intensity, Neutrals, Tone and Grain.
  - Double-click a slider to reset it; a checkbox toggles each adjustment group.
- **Photographic Styles:** 15 styles with a 2D tone/colour pad and a palette (intensity) slider, editable after capture.
- **Portrait depth:** editable after the fact.
- **Batch:** copy and paste edits.
- **iOS 27 AI (Apple Intelligence, iPhone 15 Pro and later):**
  - Clean Up with **Fast / High Quality / Auto** modes; handles larger edits.
  - **Extend** (generative outpaint).
  - **Spatial Reframing** (new viewpoint).
  - AppleInsider: "Like most AI tools, results can be inconsistent"; Reframing "can sometimes distort facial features", and one commenter called it "nightmare fuel".
  - Whether these run on-device or in Private Cloud Compute is not stated (unverified).

### 1.16 Google Photos

- **Pricing:** free. Some tools need a Pixel or Google One (unverified 2026 details).
- **Editor redesign (Android v7.44, September 2025):**
  - Tabs: **Auto** (Enhance, Dynamic, AI Enhance, which "generate multiple results"); **Actions** (Crop, Magic Eraser, Move, Best Take, Portrait blur, Pop, Sharpen, Denoise); **Markup**; **Filters** (with Sky styles); **Lighting** (Ultra HDR, Portrait light, Brightness…); **Color** (Saturation, Warmth, Tint, Skin tone, Blue tone).
  - A **search icon** finds tools.
  - Tap, circle or brush an area to get contextual **Erase / Move / Reimagine**.
  - **Auto Frame** suggests crops or generative widening.
- **"Help me edit" conversational editing:**
  - Debuted on Pixel 10, then reached all eligible US Android users (October 2025) and iOS with Nano Banana (November 2025).
  - Accepts voice or text, offers suggestion chips including "make it better", and uses **private face groups** for personalised edits such as removing sunglasses or changing expressions.
  - A "Create with AI" template carousel includes prompts like "create a professional headshot".
  - Eligibility gates: 18+, US English, Face Groups and location estimates on.
- **Reviewer framing:** conversational editing is a workaround for a buried UI. "When editing a photo with a flagship AI feature like Auto Frame requires nearly 10 button presses, the chances of an average user finding it in the Photos app are slim." — [MakeUseOf](https://www.makeuseof.com/google-photos-conversational-editing-ditch-my-mirrorless/)

### 1.17 Samsung Galaxy AI (S26, One UI 8.5)

- **Photo Assist** now takes **text prompts**: day to night, change an outfit, restore missing parts, remove spills.
- It moves objects and adds matching shadows.
- **Every AI change is kept in a history that can be reviewed step by step, adjusted or undone.**
- Creative Studio offers style transfer and sticker sets.
- Where it runs and how outputs are watermarked is not stated in the fetched sources (unverified).

### 1.18 AI-native newcomers and pro-retouch specialists

| Product | Model / pricing (2026) | What it does | Local or cloud | Notes |
|---|---|---|---|---|
| **Evoto** | Credits: most features cost 1 credit per *exported* image; editing and preview are free. From about $9.99/mo (unverified) | Per-face portrait retouch, blemish removal preserving texture, wrinkles, **glasses glare**, **stray hairs**, AI Color Match, background swap, batch sync, culling, galleries, tethering ("Evoto Instant"), video retouch | **Cloud** for most features; images deleted after processing; crop and background replacement local | Credit-per-export pricing is the core seam |
| **Aftershoot** | Flat subscriptions: Selects about $15/mo, Essentials $25, Pro $48, Max $72 (from a competitor blog; unverified) | Market leader in culling; personal AI edit profiles trained on your own catalog; retouching | **Local-first** (16 GB RAM) | Flat price is its pitch against Imagen |
| **Imagen AI** | Per photo: editing about $0.22–$0.32, culling priced per photo (unverified) | Personal AI profiles, "Talent AI" profiles from known photographers, culling, Lightroom handoff | **Cloud** | — |
| **Retouch4me** | Perpetual plugins from $145 each (Heal, Dodge&Burn, Portrait Volumes, Face Make, Skin Tone, Skin Mask, Mattifier, Crop, Eyes, White Teeth, Stray Hairs, Color Match, Fabric, Dust, Clean Backdrop). Free Frequency Separation and Color Match Free plugins. Subscriptions $169/$299/$759 a year with retouching and culling credits and cloud storage | Plugin-per-task portrait retouch | **Perpetual versions run offline** | A four-tool portrait workflow costs about $500 |
| **Magnific** (formerly Freepik) | Premium $14.50/mo, Premium+ $33.75, Pro $210 (API); credits last a year | Creative upscaler, **Precision** (non-hallucinating) 2× mode, **Relight** with 3 lights (rotation, elevation, intensity, colour) plus a reference-image light match, video upscale, Photoshop plugin | Cloud | Reviewer: "Skip Magnific when: you are a photographer who needs faithful enlargement" ([runtheeval](https://runtheeval.com/magnific-ai-2026-review-upscale-relight/)) |
| **Krea** | Free 100 units a day; Basic $9, Pro $35, Max $70, Business $200 | **Real-time canvas** (updates as you type or draw), multi-model hub (Flux Krea, ChatGPT Image, Ideogram, Imagen), enhancer, training, Nodes workflows | Cloud | AI-native design rather than photo correction |
| **Photoroom** | Free (watermark, non-commercial), Pro $7.50/mo, Max $20.99, Ultra from $82.50; API $0.02/image for background removal, $0.10 for edits | Background removal, AI scenes, **smart shadows**, batch up to marketplace templates, upscale | Cloud | Capture One 16.8 Actions route to it |
| **Clipdrop** (Jasper) | Free 20 per 24 h per tool; Pro €15/mo | Cleanup, relight, uncrop, background removal, text removal, upscale | Cloud | Still live; API folded into Jasper |
| **Claid.ai** | (unverified) | E-commerce product-photo enhancement and generation API | Cloud | Not researched in depth |
| **Google Nano Banana 2 / Pro** | Gemini app and API | Instruction editing, multi-image consistency, text rendering, SynthID on every output | Cloud | Default image model in the Gemini app since February 2026 |
| **OpenAI GPT Image 2 (ChatGPT Images 2.0)** | ChatGPT plans and API | Instruction editing, best text-in-image, multi-image consistency; 2.5 adds Sketch (unverified) | Cloud | — |
| **FLUX.1 Kontext [dev] / FLUX.2 / klein** | Open weights (licences vary; check before shipping) | Contextual in-place editing, character consistency | **Can run locally**: about 24 GB VRAM (FLUX.2 dev); klein 4B about 13–16 GB | Can "soften fine detail when working on tight masks" (Banana Designer comparison, via search) |
| **Qwen-Image-Edit** | Open weights | Instruction editing, bilingual text replacement inside images | Local about 24 GB | The strongest open option for text edits |

**What pros actually use (synthesis).**
- Wedding and event photographers pair Lightroom or Capture One with Aftershoot or Imagen for culling and a first-pass edit.
- Portrait and studio photographers pair Capture One with Evoto or Retouch4me.
- Landscape and wildlife photographers use DxO DeepPRIME, Topaz, or Lightroom Denoise.
- E-commerce uses Photoroom or Claid.
- Prompt editors (Nano Banana, GPT Image, Kontext) are used for concepts, social content and composites, **not** for faithful client deliverables. Every photographer-focused review above favours "faithful" modes, such as Magnific Precision and Topaz Standard over Wonder, for real photos.

---

## 2. Features not in Photoshop

**Columns:**
- **Class for Pro:** *must* (a Pro user expects it in a primary tool), *edge* (differentiator for some segments), *bloat* (low value for serious photo work).
- **Lite?** Whether it belongs in the simple profile.
- **Incumbent runs:** where the incumbent computes it.
- **Build:** S/M/L/XL effort for a Rust core with wgpu compute and a wasm/web UI, including model integration where relevant.

"PS partial" in a note means Photoshop or Camera Raw has something close. Check those rows against the Photoshop document.

### 2A. Library, DAM and output

| # | Feature | Found in | What it does (≤12 words) | Pro | Lite? | AI? | Incumbent runs | Build | Note |
|---|---|---|---|---|---|---|---|---|---|
| 1 | Catalog database with cached previews | Lr Classic, C1, ON1, Luminar | Index thousands of photos; browse and search without opening files | must | no | no | on-device | L | Adobe Bridge is a separate app; PS has no library. SQLite plus preview cache |
| 2 | Browse folders without importing (catalog optional) | ON1 Browse, DxO PhotoLab | Edit straight from disk folders; cataloguing is opt-in | must | yes | no | on-device | M | Best default for Lite; File System Access API in the browser |
| 3 | Sessions: self-contained per-shoot projects | C1 | Capture/Selects/Output/Trash folders travel with the job | edge | no | no | on-device | M | Reviewers cite Sessions as a reason pros stay on C1 |
| 4 | Collections and smart (rule-based) collections | Lr, C1 Smart Albums | Virtual albums, auto-filled by rating, keyword, camera rules | must | yes | no | on-device | M | Lite: plain albums only |
| 5 | Virtual copies / variants | Lr, C1 | Several independent edits share one original file | must | yes | no | on-device | S | Edits stored as parameter documents |
| 6 | Stacks (manual and by capture time) | Lr, C1 | Collapse bursts or brackets under one thumbnail | edge | no | no | on-device | S | Millisecond time stacking in Lr 15 |
| 7 | Auto-stack by visual similarity | Lr Classic 15 | Groups near-identical framings automatically | edge | no | yes | on-device | M | Image embeddings plus clustering |
| 8 | Face detection, clustering and People view | Lr, Apple Photos, Google Photos | Find, group and name people; filter library by person | edge | yes | yes | Lr Classic local; Google cloud (unverified) | L | A local version is a privacy win; model licences need care |
| 9 | Natural-language visual search | Lr desktop/web (cloud mode only), Google Photos | Search "white dog and a child" across library | edge | yes | yes | **cloud** | L | Local CLIP/SigLIP index is feasible; clear seam |
| 10 | AI auto-keywording | Lr cloud auto-tags (unverified), ON1 Keyword AI (unverified) | Suggest keywords from image content | edge | no | yes | mixed | M | Same embedding index as #9 |
| 11 | Hierarchical keywords, keyword sets, synonyms | Lr, C1 | Controlled vocabulary written to IPTC/XMP | edge | no | no | on-device | M | Bridge has keywords (PS partial) |
| 12 | Import pipeline: rename, metadata template, preset, backup | Lr, C1 | Ingest cards with naming, copyright, second-drive copy | must | no | no | on-device | M | Desktop shell needed for card access |
| 13 | Batch rename with tokens | Lr, C1, Bridge | Rename by date, sequence, metadata tokens | must | no | no | on-device | S | — |
| 14 | Watch folders / Folder Actions | ON1 2027, Lr auto-import | New files get metadata, presets, exports automatically | edge | no | no | on-device | M | Needs native shell; not in pure wasm |
| 15 | Duplicate detection on import | Lr | Skip files already in library | edge | yes | no | on-device | S | Content hash plus perceptual hash |
| 16 | Map view and GPX track geotagging | Lr Classic | Place photos on a map; sync GPS tracklogs | edge | no | no | map tiles from network | M | OSM tiles; reverse geocoding also needs network |
| 17 | Smart Previews / offline proxies | Lr | Edit while originals are disconnected | edge | no | no | on-device | M | Lossy DNG/JPEG-XL proxies |
| 18 | Multi-device edit sync | Lr cloud-based, Luminar Ecosystem, Photomator/Pixelmator iCloud | Same edits on desktop, phone, web | edge | yes | no | **cloud** | L | Parameter docs sync cheaply; originals are the hard part |
| 19 | Publish services (keep exports in sync online) | Lr Classic | Re-export changed photos to Flickr, disk, etc. | bloat | no | no | cloud | M | — |
| 20 | Compare and Survey views | Lr, C1, ON1 2027 | Side-by-side candidates with synced zoom and pan | must | yes | no | on-device | S | — |
| 21 | Focus mask / focus peaking overlay | C1, Affinity 3.2, ON1 (unverified) | Highlights sharp regions while culling or developing | must | no | no | on-device | S | Laplacian or Sobel shader |
| 22 | Books module | Lr Classic (Blurb) | Photo-book layout, PDF/Blurb export | bloat | no | no | cloud print | L | — |
| 23 | Slideshow and video export | Lr Classic (4K since 15) | Photos plus music to video | bloat | yes | no | on-device | M | WebCodecs |
| 24 | Print layouts: contact sheets, picture packages | Lr Classic, C1 Contact Sheets (16.7 beta) | Multi-image print and PDF layouts | edge | no | no | on-device | M | — |
| 25 | Client proofing galleries with favourites and comments | Lr shared albums, Luminar Spaces, Evoto | Clients pick favourites online | edge | no | no | **cloud** | L | Needs a server; outside a local-only scope |
| 26 | Plugin SDK for export, publish, metadata | Lr (Lua SDK) | Third parties extend library and export | edge | no | no | on-device | L | WASM component plugins |
| 27 | Catalog integrity check and auto-repair | Lr Classic 15.5 | Detect and repair corrupted catalog data at startup | must | no | no | on-device | S | SQLite integrity check plus journaling |
| 28 | Render to DNG with baked edits; compressed DNG | Lr 15.5, DxO 9.6 High-Fidelity Compression | Export edited or corrected raw as smaller DNG | edge | no | no | on-device | M | DNG writer; PS has none |
| 29 | Multiple simultaneous export recipes | C1 Process Recipes, Lr export presets | One click yields several sizes, formats, names | must | yes | no | on-device | S | Lite: "Instagram / Web / Print" presets |
| 30 | Medium-aware output sharpening | Lr export | Screen, matte or glossy sharpening at export | edge | no | no | on-device | S | — |
| 31 | Watermark editor on export | Lr, C1, Photomator | Text or logo watermark applied at export | must | yes | no | on-device | S | — |
| 32 | Batch pipeline: preset, enhance, crop, denoise, watermark | Photomator, Pixelmator Pro | Apply a whole recipe to many photos | must | yes | partial | on-device | M | PS Actions partial |
| 33 | Round-trip handoff raw editor ↔ pixel editor | DxO PL10 → Affinity, Lr/C1 "Edit in" | Send developed image to layers and back, versioned | must | no | no | on-device | S | In one app this is native: raw layer inside a document |

### 2B. Culling

| # | Feature | Found in | What it does (≤12 words) | Pro | Lite? | AI? | Incumbent runs | Build | Note |
|---|---|---|---|---|---|---|---|---|---|
| 34 | AI culling scores: eye focus, subject focus, exposure | Lr 15/15.3, C1 16.8 Assisted Review, ON1 2027 MAX, Aftershoot, Evoto | Flags rejects: missed focus, closed eyes, bad exposure | must | no | yes | Lr: credits (per Lightroom Queen); ON1 and Aftershoot local; Imagen and Evoto cloud | M | Face landmarks plus blur metric; fully local possible |
| 35 | Similar-shot grouping with best-frame pick | Aftershoot, ON1 2027 | Groups a sequence, proposes the strongest frame | must | yes ("best of burst") | yes | local | M | Embeddings plus quality score |
| 36 | Closed-eye / blink detection | C1, ON1 2027, Aftershoot | Flags shut eyes per detected face | must | yes | yes | local | S | Eye-aspect ratio from landmarks |
| 37 | Face-zoom culling panel | Aftershoot (unverified), Evoto (unverified) | Crops every face in frame for fast checks | edge | no | yes | local | S | — |
| 38 | Embedded-JPEG instant browsing | Lr "Embedded & Sidecar" | Show camera JPEG preview before raw decode | must | no | no | on-device | S | Preview extraction in Rust raw decoder |
| 39 | Keyboard culling with auto-advance | Lr, C1 | Rate or flag, jump to next automatically | must | no | no | on-device | S | — |
| 40 | Black / blank frame detection | C1 16.8 | Flags accidental black frames | edge | no | no | local | S | Histogram test |
| 41 | Personal AI profile learned from your past edits and picks | Aftershoot, Imagen | Mimics your culling and look on new shoots | edge | no | yes | Aftershoot local; Imagen cloud | XL | A cheaper local proxy: nearest-neighbour look transfer from your edited catalog |

### 2C. Tethering and studio

| # | Feature | Found in | What it does (≤12 words) | Pro | Lite? | AI? | Incumbent runs | Build | Note |
|---|---|---|---|---|---|---|---|---|---|
| 42 | Tethered capture | Lr Classic (Canon, Nikon, Sony, Fuji, Leica), C1, Evoto Instant | Shots land in the app as captured | edge (must for studio) | no | no | on-device | L | WebUSB + PTP in Chromium, or native shell with libgphoto2 |
| 43 | Live View and remote camera control | C1 | Focus, drive, exposure controlled from computer | edge | no | no | on-device | XL | Vendor SDK coverage is the moat |
| 44 | Next Capture Adjustments | C1 | Auto-apply style, crop, metadata to incoming frames | edge | no | no | on-device | S | Once #42 exists |
| 45 | Wireless tethering with proxy raw first | C1 16.8 (Canon R5 II, R1) | Small raw arrives first; full file backfills | edge | no | no | on-device | XL | — |
| 46 | Composition overlay in tether | C1 Overlay | Layout template over live captures | edge | no | no | on-device | S | — |
| 47 | Remote live client viewing | C1 Live (Studio) | Clients watch and rate the shoot in a browser | bloat | no | no | cloud | L | — |
| 48 | Capture naming counters and ReTether auto-reconnect | C1 | Sequential names; recovers dropped camera links | edge | no | no | on-device | M | — |
| 49 | "Actions" routing selections to external AI services | C1 16.8 Studio for Teams/Enterprise (Pixelz, Photoroom, Gemini) | Send picks to third-party services in-app | bloat | no | yes | cloud | M | Generalise as "send to local model or webhook" |

### 2D. Develop, colour, optics and restoration

| # | Feature | Found in | What it does (≤12 words) | Pro | Lite? | AI? | Incumbent runs | Build | Note |
|---|---|---|---|---|---|---|---|---|---|
| 50 | Layered raw adjustments with per-layer opacity | C1 | Stack masked adjustment layers inside raw develop | must | no | no | on-device | M | ACR has masks without layer opacity (PS partial; verify) |
| 51 | Stackable styles (additive presets) | C1 Styles | Apply several looks on top of one another | edge | yes | no | on-device | S | — |
| 52 | Color Editor with uniformity (skin smoothing) | C1 | Target hue range; compress its colour variance | must | no | no | on-device | M | ACR Point Color "Variance" similar (PS partial) |
| 53 | Speed Edit (hotkey plus scroll or drag) | C1 | Adjust any parameter without touching panels | edge | no | no | on-device | S | — |
| 54 | Smart Adjustments: normalise exposure/WB across a set | C1 | Match set to a reference frame's exposure and WB | must | no | yes (unverified) | on-device (unverified) | M | Wedding and event consistency |
| 55 | Match Look / reference-image grade transfer | C1 Match Look, Pixelmator Match Colors, Evoto AI Color Match | Copy colour and tone of any reference image | edge | yes | yes | Pixelmator local; Evoto cloud; C1 unverified | M | PS has legacy Match Color (PS partial) |
| 56 | Copy/paste AI masks with per-image recompute | Lr, C1, Evoto | Paste subject/sky masks; re-detected per photo | must | no | yes | Lr local; Evoto cloud | M | Masks stored semantically ("subject"), not as bitmaps |
| 57 | U-Point control points | DxO PhotoLab, Nik Collection | Click a point; similar nearby tones adjust | edge | yes | no | on-device | M | Bilateral or guided-filter weights on GPU |
| 58 | Lab-measured optics modules incl. softness | DxO | Per camera/lens distortion, vignetting, sharpness correction | edge | no | no | on-device (module download) | XL | The data is the moat; lensfun DB is the open substitute |
| 59 | Lens Cast Calibration / flat-field | C1 LCC | Remove colour cast and vignetting from reference shot | edge | no | no | on-device | S | — |
| 60 | Face-weighted auto exposure | DxO Smart Lighting (unverified in PL10) | Auto exposure prioritising detected faces | edge | yes | yes | on-device | S | — |
| 61 | Volume-deformation (wide-angle face) correction | DxO ViewPoint (unverified 2026) | Un-stretches people at wide-angle frame edges | edge | no | no | on-device | M | — |
| 62 | Measured film-stock emulation with grain | DxO FilmPack, Snapseed Film (3.0), Lr Film-Inspired presets | Emulate named film stocks' colour, curve and grain | edge | yes | no | on-device | M | LUTs plus grain synthesis; PS has LUTs only |
| 63 | Scanned film-negative inversion | Negative Lab Pro (Lr plugin); native tools unverified | Remove orange mask, invert, balance negatives | edge | no | no | on-device | M | (unverified) which incumbents ship it natively |
| 64 | AI depth masks (select by distance) | DxO PL10, ON1 Depth Lighting | Mask foreground or background from estimated depth | must | yes | yes | on-device | M | Depth-Anything-class model; ACR depth range needs embedded depth (PS partial; verify) |
| 65 | Multi-band sharpening | Affinity 3.2 | Sharpen only chosen spatial-frequency bands | edge | no | no | on-device | S | Laplacian pyramid in wgpu |
| 66 | Genre-specific denoise models | ON1 NoNoise AI 2027, Topaz (April 2026) | Separate models for wildlife, astro, portrait, macro | edge | no | yes | on-device | L | ACR has one Denoise model |
| 67 | Deblur / motion-shake recovery | Topaz Super Focus, ON1 Tack Sharp, Google Photos Unblur | Recover soft or shaken images | edge | yes | yes | Topaz local/cloud; Google unverified | L | PS dropped Shake Reduction (unverified; check PS doc) |
| 68 | Smart Deband plus bit-depth expansion | Photomator, Pixelmator Pro | Remove banding/blocking; promote 8-bit to smooth gradients | edge | yes | yes | on-device | M | Good fit for JPEG-heavy Lite users |
| 69 | Autopilot: suggest fixes, don't apply | Topaz Photo | Detects noise, blur, faces; proposes settings | edge | yes | yes | on-device | M | A UX pattern as much as a feature |
| 70 | Low-res face recovery | Topaz Photo/Gigapixel, Luminar Restoration | Reconstruct detail in tiny or blurry faces | edge | yes | yes | local (Topaz) | L | Check licences of GFPGAN/CodeFormer-class models |
| 71 | Content-specific upscale models | Topaz Gigapixel (Text & Shapes, Art & CG, Low Res, High Fidelity) | Choose upscaler by content type | edge | no | yes | local plus cloud | L | Real-ESRGAN variants (Upscayl proves demand) |
| 72 | Creative/generative upscale with creativity slider | Magnific, Gigapixel Redefine, Krea | Adds invented detail at a chosen strength | bloat (photo) / edge (design) | no | yes | cloud (Magnific, Krea); Topaz local/cloud | XL | Reviewers steer photographers to faithful modes |
| 73 | Old-photo restoration: cracks, stains, fade, colourise | Luminar Restoration, Affinity Colorize (Canva Premium) | One-click repair and colourisation | edge | yes | yes | Luminar local (pass-gated); Affinity cloud | L | PS Neural Filters partial (PS partial) |
| 74 | Faithful (non-hallucinating) 2× upscale mode | Magnific Precision | Enlarge without inventing texture | edge | yes | yes | cloud | M | Local Real-ESRGAN-class; offer "faithful" as the default |

### 2E. Portrait and retouch suites

| # | Feature | Found in | What it does (≤12 words) | Pro | Lite? | AI? | Incumbent runs | Build | Note |
|---|---|---|---|---|---|---|---|---|---|
| 75 | Batch per-face retouch profiles | Evoto, Retouch4me, C1 Blemish/Even Skin | Retouch every face across hundreds of photos | must (portrait) | no | yes | Evoto cloud; Retouch4me local | L | Evoto charges per export; a local version removes the meter |
| 76 | AI blemish healing that keeps pores | Evoto, Retouch4me Heal, C1 Blemish Removal | Remove spots while preserving skin texture | must | yes | yes | mixed | L | — |
| 77 | AI dodge and burn | Retouch4me Dodge&Burn | Automatic micro-contrast evening of skin | edge | no | yes | local | L | — |
| 78 | Portrait Volumes (AI contouring) | Retouch4me | Adds sculpting light and shadow to faces | edge | no | yes | local | L | — |
| 79 | Skin tone evening | C1 Even Skin, Retouch4me Skin Tone | Unify blotchy skin colour | must | yes | yes | local | M | — |
| 80 | Eyes and teeth retouch by detected region | C1 16.7, Retouch4me Eyes/White Teeth, Luminar Face AI | Brighten irises, whiten teeth automatically | edge | yes | yes | local | M | — |
| 81 | Glasses glare removal | Evoto | Remove reflections on eyeglass lenses | edge | no | yes | cloud | L | Inpainting guided by lens segmentation |
| 82 | Stray / flyaway hair removal | Evoto, Retouch4me Stray Hairs | Clean hairs over background and face | edge | no | yes | mixed | XL | — |
| 83 | Fabric wrinkle cleanup | Retouch4me Fabric | Smooth creases on clothing | edge | no | yes | local | L | — |
| 84 | Studio backdrop cleanup | Retouch4me Clean Backdrop | Remove dust, seams, stains on seamless paper | edge | no | yes | local | M | — |
| 85 | Mattifier (shine reduction) | Retouch4me | Tame hot spots and oily shine on skin | edge | yes | yes | local | M | — |
| 86 | Body reshape sliders | Luminar Body AI, Evoto | Slim or shape body automatically | bloat | no | yes | Luminar local; Evoto cloud | L | Ethically contested; keep out of Lite |
| 87 | Per-face shape sliders (eyes, face slim, smile) | Luminar Face AI, Evoto | Landmark-driven face reshaping | bloat/edge | yes | yes | mixed | M | PS Face-Aware Liquify (PS partial) |
| 88 | One-step frequency separation filter | Affinity; free Retouch4me plugin | Split texture and tone into editable layers | edge | no | no | on-device | S | PS needs a manual recipe |
| 89 | Head pose adjustment | Snapseed Head Pose | Turn a face slightly left, right, up, down | bloat | yes | yes | on-device | L | PS Smart Portrait partial |

### 2F. Compositing, effects, stacking and relighting

| # | Feature | Found in | What it does (≤12 words) | Pro | Lite? | AI? | Incumbent runs | Build | Note |
|---|---|---|---|---|---|---|---|---|---|
| 90 | Tone Brush and Live Tone Blend Groups | Affinity 3.1 | Paint tonal integration into composites, non-destructively | edge | no | no | on-device | M | — |
| 91 | Live filter layers (maskable, reorderable) | Affinity | Filters behave as editable layers | must | no | no | on-device | M | PS Smart Filters partial; native in a node-graph core |
| 92 | Procedural Texture (equation filter) | Affinity | Write per-pixel expressions as a live filter | edge | no | no | on-device | M | Natural fit for WGSL codegen |
| 93 | Astrophotography stacking with dark/flat/bias frames | Affinity | Calibrate, align, stack deep-sky frames | edge | no | no | on-device | L | PS stack modes lack calibration |
| 94 | Focus merge with source-image retouching | Affinity, Luminar, ON1 | Merge focus brackets; paint from any source | edge | no | no | on-device | L | PS Auto-Blend (PS partial) |
| 95 | Tone Mapping workspace with presets for 32-bit | Affinity | Local-contrast tone map of HDR merges | edge | no | no | on-device | M | PS HDR Toning legacy (PS partial) |
| 96 | Bloom / Glow / Orton live filter | Affinity 3 Bloom, Luminar Glow, Snapseed Glow | Soft atmospheric glow from highlights | edge | yes | no | on-device | S | — |
| 97 | Depth-aware relighting with placeable lights | Luminar Light Depth, ON1 Cinematic Depth Lighting, Magnific Relight, Clipdrop Relight, Google Portrait light | Add and move virtual lights using estimated depth | edge | yes (portrait light) | yes | Luminar local; Magnific and Clipdrop cloud | XL | Depth plus normals plus shading on GPU; reviewers praise Luminar's version |
| 98 | Relight matched to a reference image | Magnific Relight | Copy lighting direction and colour from another photo | edge | no | yes | cloud | XL | — |
| 99 | Depth-aware atmosphere (fog, haze, mist) | Luminar Atmosphere AI | Adds fog that respects scene depth | edge | yes | yes | on-device | M | Reuses depth map from #64 |
| 100 | Sunrays generator | Luminar Sunrays | Places synthetic light rays behind objects | bloat | yes | no | on-device | M | — |
| 101 | Tap an object to move or extract it | Canva Magic Grab, Google Photos Move, Samsung Photo Assist | Detected object becomes movable; hole filled | edge | yes | yes | cloud (Canva); Google and Samsung unverified | L | Segment plus inpaint; PS Content-Aware Move partial |
| 102 | Extract text in an image into editable text | Canva Magic Grab text (unverified naming) | OCR image text into a live text layer | edge | no | yes | cloud | M | — |
| 103 | Best Take: best expression per person from a burst | Google (Pixel) Best Take | Combine each person's best face across frames | edge | yes | yes | on-device (unverified) | L | — |
| 104 | Spatial reframing (new camera viewpoint) | iOS 27 Photos | Generates the scene from a slightly different angle | bloat | yes | yes | Apple Intelligence (location unverified) | XL | Reviewers report face distortion |
| 105 | Photo-to-video "Animate" | Lr mobile Animate (credits), Google Photos | Adds motion (water, clouds) to a still | bloat | yes | yes | cloud | XL | — |
| 106 | Auto-highlighted distractions, tap to remove | Apple Photos Clean Up | App marks removable objects before you ask | edge | yes | yes | Apple Intelligence (unverified) | L | ACR people distraction removal (PS partial) |
| 107 | Inpaint quality modes: Fast / High Quality / Auto | iOS 27 Clean Up | User chooses speed or reconstruction quality | edge | yes | yes | Apple Intelligence | M | Maps to a small (LaMa-class) versus large diffusion model locally |
| 108 | Fence / chain-link removal by instruction | Google Photos Help me edit | Remove fences through a stated intent | edge | yes | yes | cloud | L | ACR covers reflections and wires, not fences (verify) |

### 2G. Lite presentation, AI-native and platform

| # | Feature | Found in | What it does (≤12 words) | Pro | Lite? | AI? | Incumbent runs | Build | Note |
|---|---|---|---|---|---|---|---|---|---|
| 109 | Master Light / Color / B&W sliders with sub-sliders | Apple Photos (Mac) | One slider drives several underlying adjustments | bloat (Pro) | yes | no | on-device | S | Core Lite control |
| 110 | 2D style pad plus intensity slider | Apple Photographic Styles | Drag a dot for tone and colour; slider for strength | bloat | yes | no | on-device | S | — |
| 111 | Several auto versions to choose from | Google Photos Auto (Enhance, Dynamic, AI Enhance) | Presents multiple one-tap results side by side | edge | yes | yes | Google (location unverified) | M | — |
| 112 | Tool search inside the editor | Google Photos | Type to find any tool | edge | yes | no | on-device | S | Also a Pro command palette |
| 113 | Faves: user-pinned tool row | Snapseed 3.0 | Personal quick-access tools | edge | yes | no | on-device | S | — |
| 114 | Looks with live camera capture | Snapseed 3.0, Apple Styles | Filters applied while shooting and after | bloat | yes | no | on-device | M | Web camera plus WebGPU preview |
| 115 | Editable step stack with per-step brush mask | Snapseed (edit stack; 3.0 naming unverified) | Reopen, retune or mask any earlier step | edge | yes | no | on-device | M | Natural in a parametric pipeline |
| 116 | Content-aware Quick Actions (Subject / Sky / Background / Portrait) | Lr mobile | One-tap presets aimed at detected regions | edge | yes | yes | on-device (unverified) | M | — |
| 117 | Recommended presets from image content | Lr | Suggests looks suited to this photo | edge | yes | yes | cloud (unverified) | M | ACR Adaptive presets (PS partial) |
| 118 | Instruction editing that outputs slider changes | Lr "Edit using Describe" (Android, credits) | "make this warmer" becomes editable adjustments | must (AI-native) | yes | yes | **cloud** | L | Best fit for local: small LLM or rules, reversible |
| 119 | Instruction editing that re-renders pixels | Google Help me edit (Nano Banana), Samsung Photo Assist, ChatGPT Images 2.0, Canva AI 2.0 | Describe a change; model regenerates the image | edge | yes | yes | **cloud** | XL | Open weights need about 13–24 GB VRAM |
| 120 | Reviewable, undoable history of AI edits | Samsung Photo Assist, Lr "Flatten AI edits" | Each AI step listed, adjustable, removable | must | yes | yes | — | M | Lr's flatten exists to save credits; locally there is no need |
| 121 | Crop suggestions with optional outpaint | Google Auto Frame, Photomator ML Crop, Pixelmator Auto Crop | Suggests better framing; widens with fill | edge | yes | yes | Photomator local; Google cloud (unverified) | M (crop) / XL (outpaint) | — |
| 122 | Magic Resize to many social formats | Canva Pro | One design becomes many platform sizes | edge | yes | yes | cloud | M | Smart crop from saliency |
| 123 | Templates and mockups | Canva, Pixelmator Pro, Affinity | Start from designed layouts | bloat | yes | no | cloud content | M | — |
| 124 | Brand Kits | Canva, Affinity 3.2 | Shared fonts, colours, logos | bloat | no | no | cloud | S | — |
| 125 | Frames, borders, double exposure | Snapseed | Decorative borders; blend two photos | bloat | yes | no | on-device | S | — |
| 126 | Batch e-commerce cutouts to marketplace spec, with shadows | Photoroom Batch, Canva | Remove background, pad, centre, add shadow in batch | edge | yes | yes | cloud | M | Local segmentation models are mature |
| 127 | AI-generated product scenes | Photoroom, Canva | Replace background with a generated setting | bloat | yes | yes | cloud | XL | — |
| 128 | In-app editing coach (next-step tips) | Luminar AI Assistant | Analyses image, suggests the next edit | edge | yes | yes | (unverified) | M | — |
| 129 | Plain-language automation → reusable script | Affinity 3.2 Claude connector | Describe a repetitive job; get a macro | edge | no | yes | cloud (Claude) | M | Reviewers: early, "not a production-ready tool yet" |
| 130 | Real-time generative canvas | Krea | Image regenerates live while typing or drawing | bloat | no | yes | cloud | XL | — |
| 131 | Mood boards mixing library and outside images | Lr Firefly Mood Boards | Moodboard with AI edit experiments | bloat | no | yes | cloud | M | — |
| 132 | Zero-install browser editor, local processing, PSD round-trip | Photopea | Full layered editor in a tab; files stay local | must (for OpenPhotoshop) | yes | no | on-device (browser) | XL | Proves the model: about 1M DAU and $3M/yr |
| 133 | Embeddable and self-hostable editor | Photopea API and self-host licence | Other sites embed the editor | edge | no | no | on-device | M | Revenue line Photopea has already validated |

**Row count: 133.**

**By class for Pro:** must 30 · edge 82 · bloat 21. Rows with split labels (for example #72, #87) are counted under their first label. Lite-eligible: 63. AI: 63 (the AI count includes "yes (unverified)").

---

## 3. Lite-mode UX patterns: how simple editors present editing

### 3.1 Enumerated patterns, by product

**Snapseed 3.0**
1. **Three tabs — Looks / Faves / Tools.** Presets come first, personal favourites second, the full toolbox last.
2. **Tools grouped into four plain-language buckets:** Adjust & Correct, Retouch & Transform, Style, Creative. Users don't need to know tool names.
3. **Arc slider:** one horizontal drag changes the value, and some tools switch parameter with a vertical swipe. One-handed and gesture-first, with no visible slider forest.
4. **Faves row:** users pin the tools they actually use.
5. **Home is a grid of past edits** with a single "+" button. Re-editing is first-class.
6. **Export in a fixed top-right position**, the same everywhere.
7. **Camera with Looks applied live**, so a photo can be styled before it is taken.
8. **Editable edit stack.** Earlier steps can be reopened, retuned or brushed in or out (long-standing Snapseed behaviour; 3.0 naming unverified).

**Apple Photos**
9. **Auto wand** as the first control, on-device.
10. **Master Light / Color / B&W sliders** that expand into sub-sliders. One slider serves most users; the full set is one disclosure away.
11. **Double-click a slider to reset**, plus a **checkbox to toggle each adjustment group**. That gives before/after per group.
12. **Photographic Styles 2D pad plus intensity slider.** Two perceptual axes replace a dozen sliders.
13. **Clean Up auto-highlights "intruders"**: tap to remove, or circle or brush your own.
14. **Quality modes (Fast / High Quality / Auto)** for Clean Up in iOS 27, so the user picks speed versus care.
15. **Extend and Reframe** framed as fixes to "common photography mistakes", not as generation.
16. **Copy/Paste edits** across similar photos, the simplest form of batch.
17. **Portrait depth editable afterwards**: tap to change the focus subject, slide to change strength.

**Google Photos**
18. **Auto tab with several one-tap results** (Enhance, Dynamic, AI Enhance). The user chooses instead of trusting one auto result.
19. **Five labelled tabs:** Actions, Markup, Filters, Lighting, Color. Reviewers still found flagship tools buried (about 10 taps to Auto Frame).
20. **Tool search icon** in the carousel.
21. **Tap, circle or brush part of the photo** to get contextual actions (Erase, Move, Reimagine). Object first, tool second.
22. **"Help me edit" prompt field** with **suggestion chips** and a literal **"make it better"** option. Voice works too.
23. **"Create with AI" template carousel** of ready prompts.
24. **Pill-shaped sliders with open/close animation**; framing, aspect, flip and rotate pinned at the top.
25. **Personalisation from face groups**, for example "remove his sunglasses".

**Pixelmator Pro / Photomator**
26. **One-click ML Enhance** as the default first action.
27. **One-click Select Subject / Sky / Background**, which turns masking into a menu choice.
28. **ML Crop / Auto Straighten** as buttons, not a manual task.
29. **Batch panel** that runs preset, enhance, crop, denoise and watermark in one pass.
30. **Photos-library-native browsing.** No import step; edits sync back to the system library.
31. **Split-view compare** (Pixelmator Pro 4).

**Canva**
32. **Magic Eraser: brush or click**, one gesture.
33. **Magic Grab: detected objects become draggable layers.** The mental model is "move the thing", not "select, copy, fill".
34. **Magic Resize** to every social format from one design.
35. **Some AI outside the credit pool on "fair use"** (Eraser, Expand, BG Remover), so everyday actions don't feel metered.

**Lightroom mobile**
36. **Quick Actions panel whose buttons depend on image content.** Subject, sky, background and portrait options appear only when detected.
37. **Enhance** (scene-aware) and **Fix angle** as top-level buttons.
38. **Adaptive presets** that target detected regions (Portrait, Sky, Subject, Landscape, Blur Background).
39. **Recommended presets** ranked for this image.
40. **Edit using Describe** with style chips (Natural, Vintage, Cinematic, Light & Airy, Warm & Earthy) plus free text.
41. **Credit cost shown under the AI button** before tapping (Animate).

**Samsung Photo Assist**
42. **Text box for edits plus a step-by-step AI edit history** where each change can be reviewed, adjusted or undone.
43. **Moved objects get matching shadows automatically**, which hides the seam novices can't fix.

### 3.2 Ten best patterns for OpenPhotoshop Lite

1. **Auto with alternatives.** Offer 3–4 auto results as thumbnails rather than one opaque Auto (Google).
2. **Master sliders with disclosure.** Light, Color and B&W, each expanding into sub-sliders; double-click resets; a checkbox toggles each group (Apple).
3. **Object-first contextual actions.** Tap, circle or brush part of the image to get Erase / Move / Adjust-this / Blur-background (Google, Canva, Apple).
4. **Content-aware Quick Actions.** Show "Brighten subject / Enhance sky / Blur background" only when the model sees a subject, sky or background (Lightroom mobile).
5. **Looks with a strength slider and a 2D mood pad** (Snapseed Looks, Apple Styles).
6. **Three-tab shell with pinned Faves** and plain-language tool groups (Snapseed).
7. **Prompt box with suggestion chips** including "make it better". Locally, map it to *parametric* edits the user can inspect (Google chips, Lightroom Describe).
8. **Tool search / command palette** shared by Lite and Pro (Google).
9. **Reviewable step stack** where every step, AI or not, can be re-opened, masked or deleted, plus copy/paste edits (Snapseed, Samsung, Apple).
10. **Quality mode choice on heavy AI (Fast / Best / Auto)** plus **auto-highlighted distractions** (iOS 27 Clean Up). This maps directly to a small versus large local model.

---

## 4. AI-native UX patterns

| Pattern | Who does it | Done well or gimmick (per reviewers) | Local feasibility for OpenPhotoshop |
|---|---|---|---|
| **Object removal by brush, click or auto-highlight** | Canva Magic Eraser, Apple Clean Up, Google Magic Eraser, Lr Remove, Photopea | **Done well.** The most consistently praised AI tool ("one of the most consistently reliable tools in the suite", fast.io). Complaints are about failures and credits, not the concept | Yes. LaMa-class inpainting is small; diffusion inpainting as the "High Quality" mode |
| **Prompt edit → re-rendered pixels** | Google Help me edit, Samsung Photo Assist, ChatGPT Images 2.0, Nano Banana, Canva Magic Edit, Photopea Magic Replace | **Mixed.** Great for discoverability and fun transformations. MakeUseOf frames it as a fix for a buried UI. Canva's Magic Edit is "a useful tool for quick fixes, not a Photoshop replacement". Pros avoid it for deliverables | Only on high-VRAM machines (FLUX.2 klein 4B about 13–16 GB; Qwen-Image-Edit and FLUX.2 dev about 24 GB). Offer as optional, BYO-key or local-GPU |
| **Prompt edit → parametric sliders** | Lr "Edit using Describe" (credits), Luminar AI Assistant (suggestions) | **Promising, under-delivered.** Keeps edits editable and explainable. Adobe meters it with credits; Android only | **Yes, and cheap.** A small local LLM or a rule engine maps intent to parameter deltas. Strongest local AI-native bet |
| **Contextual task bar / object-first actions** | Google tap/circle, Canva Magic Grab, Samsung Move | **Done well for simple scenes.** Magic Grab gets "messier extractions" in busy scenes | Yes. Segmentation (SAM-class) plus inpaint |
| **Suggestion chips / template prompts** | Google ("make it better", "professional headshot"), Lr Describe styles, Photopea model picker | **Good onboarding.** Personalised templates from library analysis look gimmicky ("cartoon of me and my hobbies") | Yes for parametric chips |
| **Multiple variations** | Google Auto (Enhance/Dynamic/AI Enhance), generative tools' 3-up results | **Done well** for auto-enhance, where the choice is cheap and reversible | Yes |
| **Autopilot: suggest, don't apply** | Topaz Photo Autopilot | **Well received.** Transparent. Model choice itself confuses users (Wonder's purpose unclear) | Yes |
| **Agentic multi-step edits and automation** | Canva AI 2.0 (chains tools), Affinity 3.2 Claude connector, C1 Actions, Picsart "15+ agents" | **Early / gimmick-leaning.** Affinity's connector is "not a production-ready tool yet". Canva AI 2.0 burns allowance fastest | Partial. Script generation over a documented local operation graph is feasible; keep it inspectable |
| **Masks from content (subject, sky, people parts, depth)** | Lr/ACR, C1, DxO PL10 (depth, face parts), Affinity 3.2 Develop Object Selection | **Done well; table stakes in 2026.** Batch re-detection on paste is the pro feature | Yes (segmentation and depth models on WebGPU) |
| **Masks and search from text** | Lr natural-language search (cloud mode only); prompt editors select implicitly | Search is useful. Explicit text-to-mask in photo editors is not verified in these sources (unverified) | Yes. Open-vocabulary detector plus SAM locally; CLIP index for search |
| **Relighting with direct manipulation** | Luminar Light Depth (sliders, 3D lights), Magnific (3 lights plus reference), Google Portrait light | **Done well now.** Light Depth "worth the upgrade"; its predecessor Relight "didn't work very well" | Yes but XL: depth plus normals plus shading |
| **Before/after and per-group toggles** | Apple (checkbox per group, double-click reset), Lr/C1 split view, Pixelmator split view | **Essential**; nobody calls it gimmicky | Trivial in a parametric pipeline |
| **Undoable AI layers / AI edit history** | Samsung (reviewable, adjustable, undoable history); Lr keeps AI edits re-renderable but adds "Flatten AI edits" to save credits and a pre-export "needs AI update" warning | **Done well** (Samsung). Lr shows the cost of cloud AI: re-rendering spends credits and breaks exports | Yes. Local re-render is free, so keep every AI step live |
| **Cost shown before the action** | Lr (credits under the Animate button), Canva fair-use split | **Good practice** when metered. OpenPhotoshop has no meter, and that absence is itself the message | n/a |
| **Background AI processing** | Lr 15.3 (copy/paste/sync in background), C1 16.8 ("When Preview Is Ready", background denoise) | **Done well**: users keep culling while AI works | Yes. Web workers plus a GPU queue |
| **Faithful versus creative toggle** | Magnific Precision versus creative; Topaz Standard versus Wonder/Redefine | **Important for photographers**: reviewers steer them to faithful modes | Yes; default to faithful |
| **Spatial reframing / photo-to-video** | iOS 27 Reframe, Lr Animate, Google | **Gimmick for Pro**: "nightmare fuel" faces | Skip |
| **Real-time generative canvas** | Krea | Impressive for ideation; irrelevant to photo correction | Skip for v1 |
| **Design generation from templates** | Canva Magic Design | **Gimmick-leaning**: "design homogeneity" | Skip |
| **Generative expand** | Canva Magic Expand, Lr 15.5, iOS 27 Extend, Google Auto Frame | **Mixed**: "visible artifacts at the seam" in detailed scenes | Heavy model; optional |

**Key lessons.**
1. The AI people rely on is **removal, masking, denoise and culling**. It is correction work, and all of it can run locally.
2. **Prompt editing's real value is discoverability.** Build it as a *parametric* instruction layer that writes visible, editable adjustments. That fits Lite and runs locally.
3. **Every incumbent that meters AI adds friction**: credit anxiety, "Flatten AI edits", "needs AI update" warnings on export, failed generations that still worry users. A local editor can keep every AI step live and re-renderable at no cost.
4. **Offer faithful-first defaults and "suggest, don't apply."** Photographers distrust hallucination.
5. **Agentic automation is not production-ready anywhere yet.** An inspectable operation graph is the prerequisite, and OpenPhotoshop's Rust core can expose one.

---

## 5. Structural seams: what incumbents gate, and what could be fully local

### 5.1 What gets gated, and how

| Capability | Who gates it | Mechanism | Needs cloud technically? | Local build for OpenPhotoshop |
|---|---|---|---|---|
| Generative fill / expand / replace | Adobe Lr (Generative Expand, credits); Affinity (Canva Premium); Photopea (credits or ads); Luminar (Prime renewal $59/yr after year 1); ON1 (MAX/subscribers, cloud); Canva (allowance) | Credits, subscription, expiring AI in "perpetual" licences | No, but needs a large GPU (13–24 GB VRAM for current open weights) | Optional heavy module; LaMa-class removal as the free default |
| Object removal | Mostly *not* gated (Lr Remove free at present; Canva fair-use; Apple and Google free) | — | No | Must, local, free |
| Background removal | Canva (Pro only); Affinity (Premium); Photoroom (watermark on free) | Subscription | No | Local segmentation; S–M |
| AI culling | Lr (credits per Lightroom Queen); ON1 2027 (MAX only); Aftershoot (subscription); Imagen (per photo); Retouch4me (culling credits) | Credits, tiering, per-image | No (Aftershoot and ON1 prove local) | **Strong seam**: free local culling |
| Portrait retouch (skin, eyes, glare, hair) | Evoto (1 credit per export, cloud); Retouch4me (about $145 per plugin, or credit subscriptions) | Per-export credits, per-plugin licences | No (Retouch4me perpetual versions run offline) | **Strong seam** for portrait and wedding Pro |
| Upscale / super resolution | Topaz (subscription only); Affinity Super Resolve (Premium); Magnific (credits) | Subscription, credits | No (Pixelmator on-device since 2019; Upscayl) | Local faithful upscaler; M–L |
| Natural-language / visual search | Lr (cloud mode only) | Cloud architecture | No | Local embedding index; L |
| Parametric "edit by description" | Lr (credits, Android) | Credits | No | Local small model or rules; L |
| Colourise / restoration | Affinity (Premium); Luminar (pass-gated) | Subscription, pass | No | Local; L |
| AI Assistant / coach | Luminar (pass-gated) | Pass | No | Local; M |
| Depth masks / relighting | Luminar Light Depth free to all; DxO local; Magnific credits | Mixed | No | Local; M (masks) to XL (relight) |
| Denoise | Not gated (Lr, DxO, C1, ON1 all local) | — | No | Table stakes; L |
| Tethering | C1 (subscription premium); Lr included | Subscription | No | Native shell; L–XL |
| Multi-device sync, client galleries, Live viewing | Lr, Luminar Spaces, C1 Live, Evoto galleries | Subscription | **Yes** (a server, or peer-to-peer) | Out of scope for local v1; could reuse the suite's sync engine later |
| Non-AI features | **Pixelmator Pro Warp (Creator Studio only)** | Subscription exclusivity on a non-AI tool | No | Everything non-AI stays free; say so explicitly |
| Commercial use | **Topaz Pro tier for organisations over $1M revenue** ($499–$799/yr) | Revenue-based licence | No | Open-source licence removes this |
| Model updates | Topaz perpetual owners get no new models | Subscription for models | No | Shipping model updates with releases is the counter-position |
| Ads | Photopea (90% of revenue) | Attention | No | Not needed if not monetising; note Photopea's economics work *because* processing is local (about $12.6k/yr costs) |

### 5.2 Business-model reading

1. **"Free pro editor" is already taken by Affinity, on Windows and macOS.** Canva funds it by selling cloud AI. OpenPhotoshop cannot win on price against Affinity alone. Its openings are the things Affinity does not have:
   - Browser, Linux and zero-install (Photopea's ground, without ads).
   - **No account** (Affinity requires a Canva login, a visible complaint).
   - **Integrated DAM and culling** (Affinity has none).
   - **Local AI with no meter** (all of Affinity's AI is cloud Premium).
2. **Perpetual-with-expiring-AI is the new trap.** Luminar gives 1 year of AI; Topaz's perpetual owners get no model updates; Pixelmator's one-time buyers miss Warp. Promise "the models are part of the release, forever" and say it explicitly.
3. **Credits create UX debt.** Look at Lightroom's "Flatten AI edits" (to conserve credits), export warnings, and forum anxiety over whether Remove will be charged. Local inference removes the whole class of UI.
4. **The price-rise fatigue is real.** Capture One has had two 6% rises in two years; Lightroom went from $11.99 to $14.99; Topaz went from $99 once to $199 a year. This is where a switching message lands, but only if the tool covers the specific pro job: culling plus develop plus portrait retouch for wedding and portrait photographers, not generic "private editing".
5. **Per-image cloud pricing is the sharpest seam for volume shooters.** Evoto charges 1 credit per export and Imagen $0.22–$0.32 a photo. For a wedding of 800 delivered images, the metered cost repeats every job. The strongest wedge is a local batch portrait retouch plus culling pipeline. It is hard analysis that the per-image vendors cannot give away without cannibalising their pricing.

---

## Sources

### Adobe Lightroom
- The Lightroom Queen — What's New in Lightroom Classic 15.5 (Aug 2026): https://www.lightroomqueen.com/whats-new-in-lightroom-2026-08/
- The Lightroom Queen — What's New in Lightroom Classic 15.3 (Apr 2026): https://www.lightroomqueen.com/whats-new-in-lightroom-2026-04/
- PhotoshopCAFE — Lightroom Classic 2026 new features (Lightroom 15): https://photoshopcafe.com/lightroom-classic-2026-new-features-lightroom-15/
- PhotoshopCAFE — August 2026 update: https://photoshopcafe.com/new-features-in-lightroom-and-camera-raw-august-2026-update/
- Adobe — Lightroom plan (1TB) pricing change FAQ: https://helpx.adobe.com/ca/lightroom-cc/kb/lightroom-1tb-plan-faq.html
- TechRadar — Photoshop and Lightroom plan price hike: https://www.techradar.com/computing/creative-software/adobes-photoshop-and-lightroom-photo-plans-get-a-huge-price-hike-but-theres-a-way-to-avoid-it
- Imagen — Adobe Photography Plan pricing 2026: https://imagen-ai.com/valuable-tips/adobe-photography-plan-pricing/
- Lightroom Queen forum — Removal tool becoming a paying feature?: https://www.lightroomqueen.com/community/threads/removal-tool-generative-becoming-a-paying-feature.54470/
- Lightroom Queen forum — Generative Removed Failed: https://www.lightroomqueen.com/community/threads/generative-removed-failed.54089/
- Lightroom Queen forum — Bad (unusable) AI results and credits: https://www.lightroomqueen.com/community/threads/bad-unusable-ai-results-and-credits.53553/
- Adobe HelpX — Quick Actions in Lightroom mobile (iOS): https://helpx.adobe.com/lightroom-cc/using/actions-lightroom-ios.html
- Adobe HelpX — Auto-enhance with Quick Actions: https://helpx.adobe.com/lightroom/mobile/apply-quick-actions/auto-enhance-photos-with-quick-actions.html

### Capture One
- PetaPixel — Capture One to increase all prices by 6%: https://petapixel.com/2026/05/27/capture-one-to-increase-all-product-prices-by-6/
- Newsshooter — Capture One raises prices again: https://www.newsshooter.com/2026/06/01/capture-one-raises-prices-again-is-it-still-worth-it-or-time-to-jump-ship/
- Capture One support — 16.8 release notes: https://support.captureone.com/hc/en-us/articles/35747427882653-Capture-One-16-8-release-notes
- Capture One support — 16.7 release notes: https://support.captureone.com/hc/en-us/articles/31141690629917-Capture-One-16-7-release-notes
- PhotoTools — Capture One 16.8 Enhanced Denoise, tethering, Assisted Review: https://phototools.org/news/capture-one-16-8-enhanced-denoise-tethering
- Capture One — Capture One Pro product page: https://www.captureone.com/en/products/capture-one-pro
- Capture One support — Perpetual and subscription licences: https://support.captureone.com/hc/en-us/articles/360007791797-All-about-perpetual-and-subscription-licenses
- DPReview forums — Capture One price increase from 2 June: https://www.dpreview.com/forums/threads/capture-one-price-increase-from-2-june.4836714/
- Imagen — Capture One vs Lightroom colour grading 2026: https://imagen-ai.com/valuable-tips/capture-one-vs-lightroom-color-grading/

### Affinity (Canva)
- Affinity — Get Affinity: https://www.affinity.studio/get-affinity
- Affinity — Photo editing software features: https://www.affinity.studio/photo-editing-software
- Canva Newsroom — Introducing the all-new Affinity: https://www.canva.com/newsroom/news/all-new-affinity/
- Canva Newsroom — Why we made Affinity free: https://www.canva.com/newsroom/news/affinity-free/
- TechRadar — Affinity CEO on making it free: https://www.techradar.com/pro/software-services/affinity-ceo-reveals-why-canva-and-affinity-made-pro-design-software-free-and-what-that-means-for-creativity
- Lenscraft — Affinity 3 for Photographers: https://lenscraft.co.uk/photography-blog/affinity-3-for-photographers/
- Beyond Photo Tips — What's New in Affinity 3.2: https://www.beyondphototips.com/whats-new-affinity-3-2-for-photographers/
- CG Channel — Canva releases Affinity 3.2: https://www.cgchannel.com/2026/04/canva-releases-affinity-3-2/
- Affinity blog — March 2026 update: https://www.affinity.studio/blog/affinity-update-march-2026
- Affinity Help — Frequency separation: https://www.affinity.studio/help/retouching-retouch-frequency-separation/
- Affinity Help — Procedural Texture filter: https://www.affinity.studio/help/filters-filter-proceduraltexture/
- Threads — @douglaspmarx on the Canva login: https://www.threads.com/@douglaspmarx/post/DQe4c-tEfEB/as-long-as-i-dont-need-to-login-to-a-canva-account-to-use-affinity-ill-be-fine-i

### Pixelmator Pro, Photomator, Apple Creator Studio
- Apple — Pixelmator Pro: https://www.apple.com/pixelmator-pro/
- Apple — Apple Creator Studio: https://www.apple.com/apple-creator-studio/
- Macworld — Apple Creator Studio: https://www.macworld.com/article/3031215/apples-new-creator-studio-is-the-pro-subscription-bundle-weve-been-waiting-for.html
- TechCrunch — Apple launches Creator Studio: https://techcrunch.com/2026/01/13/apple-launches-creator-studio-bundle-of-apps-for-12-99-per-month
- 9to5Mac — Pixelmator Pro launches on iPad: https://9to5mac.com/2026/01/28/pixelmator-pro-launches-on-ipad-heres-what-the-new-app-can-do/
- 9to5Mac — Pixelmator for iOS will no longer receive updates: https://9to5mac.com/2026/01/13/apple-confirms-pixelmator-for-ios-will-no-longer-receive-updates/
- MacRumors — Pixelmator Pro launches on iPad: https://www.macrumors.com/2026/01/28/pixelmator-pro-launches-on-ipad/
- Six Colors — The real value in Creator Studio: https://sixcolors.com/post/2026/01/whats-the-real-value-in-creator-studio/
- Digital Camera World — Pixelmator Pro 4 review: https://www.digitalcameraworld.com/tech/software/pixelmator-pro-4-review
- Pixelmator — Photomator: https://www.pixelmator.com/photomator/
- Pixelmator blog — ML Super Resolution (on-device): https://www.pixelmator.com/blog/2019/12/17/all-about-the-new-ml-super-resolution-feature-in-pixelmator-pro/
- Neowin — What happens to Pixelmator and Photomator: https://www.neowin.net/news/apple-says-what-will-happen-to-the-original-pixelmator-and-photomator-apps/

### Photopea
- Shotkit — Photopea review: https://shotkit.com/photopea-review/
- Wikipedia — Photopea: https://en.wikipedia.org/wiki/Photopea
- Built Plain — Seven thousand hours at $0, then $3M a year in ads: https://www.builtplain.com/photopea-free-app-ad-revenue/
- Failory — Photopea founder interview: https://www.failory.com/interview/photopea
- checkthat.ai — Photopea pricing: https://checkthat.ai/brands/photopea/pricing
- AI Trace — Photopea Magic Replace: https://www.aitrace.org/company/photopea/practice/e5ab6701-fd7f-4711-93aa-f5f2a67b447b
- digi-tools.info — Photopea review: https://digi-tools.info/articles/photopea-review

### Luminar Neo
- Skylum — Luminar pricing: https://skylum.com/luminar/pricing
- PetaPixel — Skylum Fall Update 2025: https://petapixel.com/2025/10/28/skylums-fall-update-supercharges-luminar-neo-with-even-more-ai/
- Fstoppers — Luminar Neo Fall Update review: https://fstoppers.com/artificial-intelligence/review-luminar-neo-fall-update-winner-some-powerful-new-tools-715721
- Trustpilot — Skylum reviews: https://www.trustpilot.com/review/skylum.com
- Digital Photo Mentor — Luminar Neo Fall Update 2025: https://www.digitalphotomentor.com/luminar-neo-fall-update-2025/

### DxO
- DxO shop — PhotoLab 10 Elite: https://shop.dxo.com/en/dxo-photolab-10-elite.html
- DxO — Introducing PhotoLab 10: https://www.dxo.com/en/news/introducing-photolab-10
- Life after Photoshop — PhotoLab 10 review: https://lifeafterphotoshop.com/dxo-photolab-10-review/
- DxO — DeepPRIME XD3: https://www.dxo.com/en/news/deepprime-xd3-fourth-generation/
- Life after Photoshop — PhotoLab 9.6 update: https://lifeafterphotoshop.com/dxo-photolab-9-6-update-deepprime-xd3-compressed-dng-files-and-feathered-ai-masks/

### ON1
- ON1 — What's New in Photo RAW 2027: https://www.on1.com/products/photo-raw/whats-new/
- ON1 — Buy Photo RAW: https://www.on1.com/products/photo-raw/buy/
- PhotoWorkout — ON1 Photo RAW 2026 review: https://www.photoworkout.com/on1-photo-raw-review/
- DPReview forums — ON1 Raw 2026 review: https://www.dpreview.com/forums/threads/on1-raw-2026-review.4823008/
- Cameraderie — ON1 Photo RAW 2026: https://cameraderie.org/threads/on1-photo-raw-2026.59815/

### Topaz Labs
- Topaz Labs — Pricing: https://www.topazlabs.com/pricing
- Digital Production — Topaz drops perpetual licences: https://digitalproduction.com/2025/10/02/topaz-drops-perpetual-licences-bets-on-subscriptions-with-studio-launch/
- Topaz docs — Topaz Photo release summary: https://docs.topazlabs.com/topaz-photo/release-summary
- Silent Peak Photo — Topaz Photo AI review: https://silentpeakphoto.com/photo-editing-apps/photo-editing-app-reviews/topaz-photo-ai-review/
- Sammapix — Topaz Gigapixel pricing and free alternatives: https://www.sammapix.com/blog/best-free-topaz-gigapixel-alternatives-2026
- AlternativeTo — Topaz Gigapixel alternatives: https://alternativeto.net/software/a-i-gigapixel

### Corel, Canva, Picsart
- Aiarty — PaintShop Pro review 2026: https://www.aiarty.com/edit-photo/corel-paintshop-pro-review.htm
- PaintShop Pro — official: https://www.paintshoppro.com/en/
- eesel — Canva AI pricing 2026: https://www.eesel.ai/blog/canva-ai-pricing
- Style Factory — Canva Pro vs Free: https://www.stylefactoryproductions.com/blog/canva-pro-vs-free
- fast.io — Canva AI review 2026: https://fast.io/resources/canva-ai-review-2026/
- Flowith — Picsart pricing 2026: https://flowith.io/blog/picsart-pricing-free-vs-plus-vs-pro-plan/

### Mobile / Lite
- 9to5Google — Snapseed 3.0 on iPhone: https://9to5google.com/2025/06/12/snapseed-3-0-update-iphone/
- 9to5Google — Snapseed redesign coming to Android: https://9to5google.com/2026/01/06/snapseed-android-redesign/
- gHacks — Snapseed 3.0 for iOS: https://www.ghacks.net/2025/06/16/google-releases-snapseed-3-0-for-ios-with-new-design-and-features/
- 9to5Google — Google Photos editor redesign on Android: https://9to5google.com/2025/09/05/google-photos-editor-redesign-android/
- TechCrunch — Google Photos redesigned editor: https://techcrunch.com/2025/05/28/google-photos-debuts-redesigned-editor-with-new-ai-tools
- Google blog — Conversational editing on Android: https://blog.google/products-and-platforms/products/photos/android-conversational-editing-google-photos/
- 9to5Google — Help me edit on iOS with Nano Banana: https://9to5google.com/2025/11/11/google-photos-help-me-edit-ios-nano-banana/
- MakeUseOf — Google Photos conversational editing: https://www.makeuseof.com/google-photos-conversational-editing-ditch-my-mirrorless/
- MacRumors — iOS 27 Photos app guide: https://www.macrumors.com/guide/ios-27-photos-app/
- AppleInsider — Apple Intelligence Photos editing in iOS 27: https://appleinsider.com/articles/26/06/09/apple-intelligence-gives-photos-in-ios-27-its-biggest-editing-upgrade-in-years
- Apple Support — Adjust light, exposure and color in Photos for Mac: https://support.apple.com/guide/photos/adjust-light-exposure-and-color-pht806aea6a6/mac
- Macworld — 7 Apple Photos tools: https://www.macworld.com/article/3034108/these-7-apple-photos-tools-will-elevate-your-iphone-photography-game.html
- SamMobile — Galaxy S26 Photo Assist: https://www.sammobile.com/news/what-can-the-upgraded-galaxy-s26-photo-assist-do/
- GadgetGuy — Samsung Photo Assist hands-on: https://www.gadgetguy.com.au/samsung-galaxy-s26-photo-assist-editing-hands-on/

### AI-native and pro-retouch specialists
- Evoto — official: https://www.evoto.ai/
- Aftershoot — Aftershoot vs Imagen: https://aftershoot.com/blog/aftershoot-vs-imagen-comparison/
- Imagen — Imagen AI vs Aftershoot: https://imagen-ai.com/valuable-tips/imagen-ai-vs-aftershoot/
- Retouch4me — Pricing: https://retouch4.me/pricing
- The Next Web — Freepik rebrands as Magnific: https://thenextweb.com/news/freepik-rebrands-as-magnific
- PR Newswire — Freepik becomes Magnific: https://www.prnewswire.com/news-releases/freepik-becomes-magnific-hits-230m-arr-and-introduces-the-no-collar-creative-economy-302755376.html
- Run the Eval — Magnific AI 2026 review: https://runtheeval.com/magnific-ai-2026-review-upscale-relight/
- The Rundown — Krea: https://www.therundown.ai/tools/krea-ai
- eesel — Photoroom pricing: https://www.eesel.ai/blog/photoroom-pricing
- aipedia — Clipdrop review (June 2026): https://aipedia.wiki/tools/clipdrop/
- TechCrunch — Nano Banana 2: https://techcrunch.com/2026/02/26/google-launches-nano-banana-2-model-with-faster-image-generation/
- Google blog — Nano Banana 2: https://blog.google/innovation-and-ai/technology/ai/nano-banana-2/
- OpenAI — Introducing ChatGPT Images 2.0: https://openai.com/index/introducing-chatgpt-images-2-0/
- Wikipedia — GPT Image: https://en.wikipedia.org/wiki/GPT_Image
- Thunder Compute — Best open-source image generation models 2026 (VRAM): https://www.thundercompute.com/blog/best-open-source-image-generation-models
- builderai.tools — Instruction image editing 2026: Kontext vs Qwen vs Step1X: https://builderai.tools/blog/ai-image-editing-flux-kontext-qwen-image-edit-step1x
- Banana Designer — model comparison: https://www.bananadesigner.com/pages/product/compare/model-comparison

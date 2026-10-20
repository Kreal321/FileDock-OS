# Phase14 pinned Linux image review — 2026-10-20

Source: final UI + bounded-scroll change, based on commit2b70b31; all61behavior cases passed in
capture mode before review. Ubuntu24.04 / Playwright1.64.0 / Chromium156.0.8078.4 revision1248 /
licensed local Inter4.1. Container pinned digest:
`mcr.microsoft.com/playwright@sha256:06a9939e57531807f8d5fd76ce44b53165ffb7d7501d87ab10e285c20b1e971f`.

All65captures were inspected via numbered visual QA sheets.45final images are byte-identical to
those already inspected;20changed images were reviewed again. Initial mobile tables with vertical
letters were rejected, fixed and recaptured; those initial images were not accepted as baselines.
Desktop single-row Events controls, centered fitting dialogs, six expanded folder sections, neutral
disclosure parents, three Settings children, account/token sections, truthful health and offline
assets follow the approved guide. Mobile tables intentionally use scoped horizontal scrolling.
Full-page captures can extend beyond the viewport backdrop; centering is measured against the
actual viewport, not the stitched full-page image. Synthetic recovery keys are masked.

49reviewed PNGs establish native Linux baselines in the two suite snapshot directories; historical
Darwin files remain unchanged. The subsequent default-mode comparison run is recorded in the
acceptance document. No --update-snapshots blanket approval was used.

| Capture | SHA-256 | Image-specific review |
| --- | --- | --- |
| `log-details-log-preview-and-complete-details-at-1440px/log-details-1440.png` | `9e64459d6dfdc2616aa7dc9e04da08cdafd06919ada5993643367b1e83c753ce` | Reviewed layout, hierarchy, selected owner and readable content. |
| `log-details-log-preview-and-complete-details-at-1440px/logs-1440.png` | `3c7c35e293dbb398202b4d60bf44c27cec661fc5445f2df9d3df288e7fb4339f` | Reviewed layout, hierarchy, selected owner and readable content. |
| `log-details-log-preview-and-complete-details-at-360px/log-details-360.png` | `3ac286a606384a36e9333bf9dbd8f205684d758179d8e7ff4431781a81fd1ea1` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `log-details-log-preview-and-complete-details-at-360px/logs-360.png` | `feef97260cefd22ef98daea4cf59dfdcb970382f2ffaf08419b186066cd6c570` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-addon-Events-toolbar-an-b2050-ewport-dialog-geometry-1440/dialog-1440.png` | `6b140f2a4f3e010e81fe03288ab6c58ed5e3668bc08778aec623d553bfdf7fe0` | Reviewed viewport-relative modal, backdrop, bounded body and reachable action footer; automated center checks pass. |
| `ui-addon-Events-toolbar-an-b2050-ewport-dialog-geometry-1440/events-1440.png` | `4266b5e8a4b0dc573d84670d13aef77477084fdd733ce4b55d65ed86441e2784` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-addon-Events-toolbar-an-f2fa4-iewport-dialog-geometry-360/dialog-360.png` | `16a79af58f9d2ab0ccf6c09602607269c4329b7a67292e5224086aa08a0174dc` | Reviewed viewport-relative modal, backdrop, bounded body and reachable action footer; automated center checks pass. |
| `ui-addon-Events-toolbar-an-f2fa4-iewport-dialog-geometry-360/events-360.png` | `7f2e04f3087d6e192c387b03850597d9e5d7668cfb55dbd3593530dc6b37eebb` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-addon-six-folder-sectio-e96e5-ckage-review-are-full-pages/folder-editor.png` | `7c21995adf7bb5951914dbfe8c68a88ce502ff0fc57622e9582fef6b146ec558` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-browser-transpor-2f2de--retains-read-only-controls/browser-outage.png` | `c59f8ffd8c2d631e7e6038787588521d2b31a025fc75e73901d484dc1ce41a9e` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-current-screens-at-1440px/dashboard-1440.png` | `c67693a27a8fc0d9f17a8189cb71e59714f315502be5f08f1ede12833c174683` | Reviewed actual synthetic state: health/failure/cache/unknown distinctions remain explicit; no fabricated protection. |
| `ui-design-current-screens-at-1440px/settings-1440.png` | `4301501e6b57295ab9c3c3cb517dfc42abdca6b63bfd68345b0d9297d10a6ca6` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-current-screens-at-1440px/settings-administrators-1440.png` | `ae22551f056c81309c0d900cd5e781dadbcaa389871b417bb7e30aef0dcda400` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-current-screens-at-1440px/settings-audit-1440.png` | `0fc5ba9715b23895c83b6e051cef5bf59ea5988c6e664f7cbacfa1e8ca467960` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-current-screens-at-1440px/settings-network-1440.png` | `bfd4b2e52858469e2ab813ae4ebc0aee79355bf5ffa991bd76b15fabe70569a1` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-current-screens-at-1440px/settings-operations-1440.png` | `3bebb4687b5ea2955722d0ba03faf3c3e4f840399f52c50963ebba68015d3849` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-current-screens-at-1440px/settings-operations-op-one-1440.png` | `0195d2215c6983fb88e95fe21b4566951aac61b5b44038523e23ce75dd5e281b` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-current-screens-at-1440px/settings-tokens-1440.png` | `40d93123da8a13320a6e34a73f15ad088815fff7824b4113494b8d022d021e50` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-current-screens-at-360px/dashboard-360.png` | `e5561838ea06fbc2782f94176f736afb69c4273416120bb368fd90539bb08a53` | Reviewed actual synthetic state: health/failure/cache/unknown distinctions remain explicit; no fabricated protection. |
| `ui-design-current-screens-at-360px/settings-360.png` | `2795180ac4f8cc7db2997cdad67f8e316bd4659ea2753fb597facc5f2a81362e` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-current-screens-at-360px/settings-administrators-360.png` | `5c5819f486e3af1cf48afdca2ac3617f72a8dc7826d9bf41fba85ca88b44784c` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-current-screens-at-360px/settings-audit-360.png` | `41568e7b8f4e1c33b3a3f331e3f2076f28e54816e8bbb955f7c0a8ec030475ee` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-current-screens-at-360px/settings-network-360.png` | `84a31cb6447edd1754f845395ccf5a12cebb453eb7271113be3b218da8c98966` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-current-screens-at-360px/settings-operations-360.png` | `c6ad6dd16dd36fe2c8e631b89bbfd7d88a07adbf902c41bb05809e101038a705` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-current-screens-at-360px/settings-operations-op-one-360.png` | `08f6146f416306539f037e6ca8eb45ce76ccb61694e90f499ac0a6bff32d3d82` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-current-screens-at-360px/settings-tokens-360.png` | `a83c0667df6f1303c52800d69629b4b7df5f9df507c5b5a7f4476ee587132eec` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-public-gate-401-at-1440px/gate-401-1440.png` | `e657501c664d6b9600d27e288547ba7f65be869eda188249157c650c314e6629` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-public-gate-401-at-360px/gate-401-360.png` | `c666c9b5fb131e73f925085a104e11b4725135cb17639fd1e08060fbbac60ea6` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-public-gate-423-at-1440px/gate-423-1440.png` | `3184d094a1885e04f21833d734923b0d4e2796e6afde0465fcfd2fbab5b00438` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-public-gate-423-at-1440px/setup-admin-1440.png` | `cdeefa288b88c15fb0b09cfcb5e21ea0ee0a57d36d4cff9b1771d304f8b1efa3` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-public-gate-423-at-1440px/setup-mismatch-1440.png` | `1562448eb07575950c52698d1c55e228ddd6bb501847b2978c88a0067d814aa5` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-public-gate-423-at-1440px/setup-recovery-1440.png` | `027b1203814bcd2da4e60ceb0881bb900ca19eb7f54aff29605ceda9600685ab` | Reviewed masked synthetic key, download action and honest reauthentication state. |
| `ui-design-public-gate-423-at-360px/gate-423-360.png` | `90066d6167a110a6c3dfacb0fdbbd0d91d78fd4ec49b51eaa3037d65d8e3e159` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-public-gate-423-at-360px/setup-admin-360.png` | `b5ac432e5d4445684f33f506204dda6acf299caa12728749513b6fc9e0f72b0b` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-public-gate-423-at-360px/setup-mismatch-360.png` | `fc2f26b5bc014302a5cf9964fe25eec042c33508e722c88d299fbe2bac133b5a` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-public-gate-423-at-360px/setup-recovery-360.png` | `807abba6e6cb7bce7732a91ae2634497a7820f0352b1c1f02c7b95e47db4b202` | Reviewed masked synthetic key, download action and honest reauthentication state. |
| `ui-design-public-gate-503-at-1440px/gate-503-1440.png` | `c7db835ce9c1a3d90cd2bea810bba6f693ddcca279f8a68b0e879c8c33798f38` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-public-gate-503-at-360px/gate-503-360.png` | `ed80ca0c60a3e5232d84bdcaeb66b35f82eb4379d44c12edc502717a77d4c82a` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-read-only-list-states-empty/audit-empty.png` | `8e6161cb6b5c6e94cf0ec10b7eec683f659a44d9ab4032401010138daee629ef` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-read-only-list-states-empty/operations-empty.png` | `51cca6729a70db70bcd81d994993069f63f86dea4318032a5841a84a796a9208` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-read-only-list-states-error/audit-error.png` | `ca80fd60f2a11c628a1adeb46f75110b8e3f37f20ca9c276c784cfd29a74841c` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-read-only-list-states-error/operations-error.png` | `54bd79906c2b4983f426e95009a1ee24d777178207e4c4225eadcc44dbb9ea55` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-shared-components-and-confirmation-at-1440px/components-1440.png` | `bb0d04420ec9bac881cfe4474e2e49583b5a49f740b52587d80eb69f998f4bf3` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-design-shared-components-and-confirmation-at-1440px/confirmation-1440.png` | `f402237a2a916a0dfcce87b4d6672efe2d0a2c8207a042c0358c7fc247ee4f3a` | Reviewed viewport-relative modal, backdrop, bounded body and reachable action footer; automated center checks pass. |
| `ui-design-shared-components-and-confirmation-at-360px/components-360.png` | `c086fb723cf4f2b6e6d22525287d00a6d64c9eaca2447063565710054cb162d8` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-design-shared-components-and-confirmation-at-360px/confirmation-360.png` | `42952364e34d2151ee38a93cf2fc952d0c990d9740d0df3ec3a3ed3ad6271662` | Reviewed viewport-relative modal, backdrop, bounded body and reachable action footer; automated center checks pass. |
| `ui-design-truthful-dashboard-cached/dashboard-cached.png` | `96958879cc8c82dc268001ca22607cf8fa960f87afddf666d85fc49b282eceb2` | Reviewed actual synthetic state: health/failure/cache/unknown distinctions remain explicit; no fabricated protection. |
| `ui-design-truthful-dashboard-degraded/dashboard-degraded.png` | `133a6e5f11557b5d7a885246dd5ebc7b0cebb418c809ef976c73811c98ac9fcc` | Reviewed actual synthetic state: health/failure/cache/unknown distinctions remain explicit; no fabricated protection. |
| `ui-design-truthful-dashboard-empty/dashboard-empty.png` | `7224b05a85deeaac5537e119788c9e1378ca0df942f30a6237b3e68b81f40927` | Reviewed actual synthetic state: health/failure/cache/unknown distinctions remain explicit; no fabricated protection. |
| `ui-design-truthful-dashboard-failed/dashboard-failed.png` | `f4274f93f811c694858114cb95d94705eec3857300ec06804127d71970a12eec` | Reviewed actual synthetic state: health/failure/cache/unknown distinctions remain explicit; no fabricated protection. |
| `ui-design-truthful-dashboard-partial/dashboard-partial.png` | `3ba40ec0ce9dcf70a2729668a9f198b19b8fbfe66828b05d5a5016dcb2d5cec0` | Reviewed actual synthetic state: health/failure/cache/unknown distinctions remain explicit; no fabricated protection. |
| `ui-design-truthful-dashboard-stopped/dashboard-stopped.png` | `3f22ede7c78fde2e50868fad41a1a233e213ae7a7ccfa3ad479edd284a870176` | Reviewed actual synthetic state: health/failure/cache/unknown distinctions remain explicit; no fabricated protection. |
| `ui-design-truthful-dashboard-unknown/dashboard-unknown.png` | `4404ce3b669f1939b893a161fa707c9c830c969b68495eacfab1d9115e67ac85` | Reviewed actual synthetic state: health/failure/cache/unknown distinctions remain explicit; no fabricated protection. |
| `ui-layout-increment-dashbo-2a3f6-ain-shared-density-at-360px/dashboard-360.png` | `47578e5e255fad7614a4b465e0dbcfac79ce3520a4f754f3da33bc2556760355` | Reviewed actual synthetic state: health/failure/cache/unknown distinctions remain explicit; no fabricated protection. |
| `ui-layout-increment-dashbo-2a3f6-ain-shared-density-at-360px/public-401-360.png` | `c666c9b5fb131e73f925085a104e11b4725135cb17639fd1e08060fbbac60ea6` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-layout-increment-dashbo-2a3f6-ain-shared-density-at-360px/public-423-360.png` | `90066d6167a110a6c3dfacb0fdbbd0d91d78fd4ec49b51eaa3037d65d8e3e159` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-layout-increment-dashbo-41159-in-shared-density-at-1440px/dashboard-1440.png` | `66fa0caff393c8a41a768f0948f1e84a0537c663048dc0b8c76e5891c50fd43e` | Reviewed actual synthetic state: health/failure/cache/unknown distinctions remain explicit; no fabricated protection. |
| `ui-layout-increment-dashbo-41159-in-shared-density-at-1440px/public-401-1440.png` | `e657501c664d6b9600d27e288547ba7f65be869eda188249157c650c314e6629` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-layout-increment-dashbo-41159-in-shared-density-at-1440px/public-423-1440.png` | `3184d094a1885e04f21833d734923b0d4e2796e6afde0465fcfd2fbab5b00438` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-layout-increment-first-UI-increment-at-1440px/add-administrator-1440.png` | `311b2785385ccd148755e5a56892e89be20c418cbfb8a752954b3fe44b4cd443` | Reviewed viewport-relative modal, backdrop, bounded body and reachable action footer; automated center checks pass. |
| `ui-layout-increment-first-UI-increment-at-1440px/administrators-1440.png` | `d615e2e60d36ee1c5c842263b87a9a997c531a8b6df75687a522561cf4de8588` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-layout-increment-first-UI-increment-at-1440px/settings-1440.png` | `d511e1bc7cb1bd87057f82d57d9784e5edf0ed503ac93ee326bb54c4339a329f` | Reviewed layout, hierarchy, selected owner and readable content. |
| `ui-layout-increment-first-UI-increment-at-360px/add-administrator-360.png` | `73acb5b58df6cb420a40df888705d601aed76671271f275d0c76c9cafa9a39ad` | Reviewed viewport-relative modal, backdrop, bounded body and reachable action footer; automated center checks pass. |
| `ui-layout-increment-first-UI-increment-at-360px/administrators-360.png` | `e2ddd9fdad7d1dbf92f9d7c034cb8395389e09aae27716656858c02b96ff8864` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |
| `ui-layout-increment-first-UI-increment-at-360px/settings-360.png` | `ea0589460a89edac0436307a0109d2c755ef04ad426b8fbdd520abd60883e047` | Reviewed narrow reflow; tables scroll in their region instead of splitting headers/statuses into vertical letters. |

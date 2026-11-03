# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.5](https://github.com/forkline/robotlb/tree/0.7.5) - 2026-11-03

### Build

- deps: Update Rust crate async-trait to v0.1.92 (#97) ([6eb5bfa](https://github.com/forkline/robotlb/commit/6eb5bfa987883ade843510b993c748ca93ea9c49))
- deps: Update actions/checkout action to v7 (#106) ([394d82e](https://github.com/forkline/robotlb/commit/394d82ef9a0a6248e76cbb10fad8f7835269cae7))
- deps: Update Rust crate tikv-jemallocator to 0.7 (#103) ([69d40bd](https://github.com/forkline/robotlb/commit/69d40bd226b327bddd4664392a52832e1df89390))
- deps: Update Rust crate tower-http to 0.7.0 (#104) ([8628913](https://github.com/forkline/robotlb/commit/8628913a7babc4f461c6ccd63e653c43d69017aa))
- deps: Update Rust crate hyper-util to v0.1.21 (#95) ([50007a4](https://github.com/forkline/robotlb/commit/50007a41f799ee9c338bf507f7dd4e533e12d3df))
- deps: Update Rust crate futures to v0.3.34 (#98) ([af48dec](https://github.com/forkline/robotlb/commit/af48dec75c6e051235f98420b58d9dd0dba5de86))
- deps: Update pre-commit hook alessandrojcm/commitlint-pre-commit-hook to v9.27.0 (#99) ([d0d767a](https://github.com/forkline/robotlb/commit/d0d767aee54401b2974b989da287b7b4b7f8e039))
- deps: Update actions-rust-lang/setup-rust-toolchain action to v2 (#105) ([75f5338](https://github.com/forkline/robotlb/commit/75f5338691a00a72c1ee5806ddf0dfe804b8cd6f))
- deps: Update actions/setup-python action to v7 (#107) ([b60708e](https://github.com/forkline/robotlb/commit/b60708ed02a1939fd72c1bbc00aa9d61f708f309))
- deps: Update Rust crate hcloud to 0.26.0 (#100) ([610b30f](https://github.com/forkline/robotlb/commit/610b30f9a99ec2c1136ba22368d4c242301ba74c))
- deps: Update Rust crate kube to v4 (#108) ([3572ba5](https://github.com/forkline/robotlb/commit/3572ba57dc9db520dc1ff91a6de9376e148effb0))
- deps: Update rust Docker tag to v1.99.1 (#111) ([4ab2f86](https://github.com/forkline/robotlb/commit/4ab2f8642f7a680dd32b0cdf1176af044a5e1e90))
- deps: Update docker/setup-buildx-action action to v4 (#113) ([a2df046](https://github.com/forkline/robotlb/commit/a2df04699a47dca7c3db548dc27f3fa40ab1b9d0))

## [0.7.4](https://github.com/forkline/robotlb/tree/0.7.4) - 2026-10-14

### Fixed

- renovate: Load forkProcessing from renovate.json (#77) ([59867c0](https://github.com/forkline/robotlb/commit/59867c02808d6a501f87b108db5ccaf6bfd1bcce))

### Build

- deps: Update Rust crate hyper to v1.12.0 (#85) ([db627dc](https://github.com/forkline/robotlb/commit/db627dcca6049ef81690c62d942c6e4d645205c2))
- deps: Update Rust crate kube to v3.1.0 (#86) ([883a477](https://github.com/forkline/robotlb/commit/883a477ea46c33508a3f2aac092195038a3f8201))
- deps: Update Rust crate clap to v4.6.7 (#84) ([48efe68](https://github.com/forkline/robotlb/commit/48efe68a816159cd59da1ab7d3d6b4987ede95bf))
- deps: Update rust Docker tag to v1.99.0 (#88) ([3c27cdf](https://github.com/forkline/robotlb/commit/3c27cdf53ee47a2456e096f4c6a276dc07929c47))
- deps: Update Rust crate http-body-util to v0.1.5 (#80) ([9fcf6d7](https://github.com/forkline/robotlb/commit/9fcf6d7edb8ec4dce827e38218b1ecfecd4262ac))
- deps: Update Rust crate tokio to v1.53.3 (#87) ([2440419](https://github.com/forkline/robotlb/commit/2440419a62f84b0de6b7dab25ef0f09c765290bf))
- deps: Update Rust crate tokio-util to v0.7.20 (#81) ([bf19f59](https://github.com/forkline/robotlb/commit/bf19f5905d5378b26e209ab8bd6b899e8b60de28))
- deps: Update Rust crate tracing-subscriber to v0.3.23 (#82) ([b214490](https://github.com/forkline/robotlb/commit/b214490f98ba6a38e242e6e2ec8c93fa4506c2fa))
- deps: Update opentelemetry (#83) ([aebf922](https://github.com/forkline/robotlb/commit/aebf922e6633115c28fb1a8a5e1215dcbc339cbb))
- deps: Update azure/setup-helm action to v5 (#89) ([da5ea55](https://github.com/forkline/robotlb/commit/da5ea55497eecfc3a67ef5b2b050e73ae464f765))
- deps: Update docker/build-push-action action to v7 (#90) ([8cee7bb](https://github.com/forkline/robotlb/commit/8cee7bb7f1f40157aebc3c925c3036977c96c466))
- deps: Update docker/login-action action to v4 (#91) ([9521a6d](https://github.com/forkline/robotlb/commit/9521a6da4c51456197318da191af9201adbb1c58))
- deps: Update docker/setup-buildx-action action to v4 (#92) ([c616c52](https://github.com/forkline/robotlb/commit/c616c52e161799964d9ad9dd5c059e620810cac7))
- deps: Update docker/setup-qemu-action action to v4 (#93) ([2d4c6a9](https://github.com/forkline/robotlb/commit/2d4c6a92f603b7c60cb470b8c880aa606ea26069))

## [0.7.3](https://github.com/forkline/robotlb/tree/0.7.3) - 2026-10-13

### Added

- helm: Support revision-aware topology spread constraints (#75) ([e382a87](https://github.com/forkline/robotlb/commit/e382a878c3262983fbbca358ffdc82eb6eb4d5a2))

## [0.7.2](https://github.com/forkline/robotlb/tree/0.7.2) - 2026-10-13

### Fixed

- Transparently recover pre-0.7 load balancers after upgrade (#73) ([c4a0996](https://github.com/forkline/robotlb/commit/c4a09961915a69cff85fb06ee4f7f8d2bd14c85a))

## [0.7.1](https://github.com/forkline/robotlb/tree/0.7.1) - 2026-10-13

### Fixed

- controller: Correct extracted module visibility and recorder initialization ([cb87115](https://github.com/forkline/robotlb/commit/cb8711547183809c978d48b46b1f491cca66b8b8))
- controller: Correct extracted module visibility and recorder initialization ([0ffcaf3](https://github.com/forkline/robotlb/commit/0ffcaf317427010d46e57e75a432180b932c9590))
- controller: Correct extracted module visibility and recorder initialization ([83d5bd0](https://github.com/forkline/robotlb/commit/83d5bd02d91f8d42fca035142d597b970c14d4c1))
- controller: Import reconciliation fingerprint memo ([fe632b0](https://github.com/forkline/robotlb/commit/fe632b0453ca449e46aef5d5c508fd98c6ef879e))
- controller: Use explicit imports and satisfy strict Clippy policy ([4f3a1b1](https://github.com/forkline/robotlb/commit/4f3a1b1cca5e73170b5951ec8017323ff0b6bff3))
- controller: Use explicit imports and satisfy strict Clippy policy ([a101b58](https://github.com/forkline/robotlb/commit/a101b58685d61f32870c098c78bb451caf5f3bff))
- controller: Use explicit imports and satisfy strict Clippy policy ([f6e0986](https://github.com/forkline/robotlb/commit/f6e0986b9a15d3bc618a7852866e07c8bfa9c017))
- controller: Clean unused imports and move explicit dependencies ([2ff5572](https://github.com/forkline/robotlb/commit/2ff55727ee1c9f16dcc332bff873f25d12890c1d))
- controller: Clean unused imports and move explicit dependencies ([a7736c0](https://github.com/forkline/robotlb/commit/a7736c03541289fc59bd58c19656ca3033d9c27a))
- controller: Apply CI rustfmt and kube Resource import ([a0fa3b3](https://github.com/forkline/robotlb/commit/a0fa3b3f8070cc3c9c081d00768edd1517c19d9f))
- controller: Apply CI rustfmt and kube Resource import ([ada0f1a](https://github.com/forkline/robotlb/commit/ada0f1ae9bfee046618b5116a7fd171119afbc16))
- controller: Apply CI rustfmt and kube Resource import ([9dc1345](https://github.com/forkline/robotlb/commit/9dc134535e7237205061c129754abeb6c42d6b7e))
- controller: Apply CI rustfmt and kube Resource import ([9bc5db7](https://github.com/forkline/robotlb/commit/9bc5db74fbdb778f284cd2f6a28032f8a3f95eba))
- controller: Use explicit imports and satisfy strict Clippy policy ([43c995c](https://github.com/forkline/robotlb/commit/43c995c7e3107eac0519a4550a3402eb7e695cd2))
- controller: Use explicit imports and satisfy strict Clippy policy ([8cd2f14](https://github.com/forkline/robotlb/commit/8cd2f142b58aa8fe868c34953bb302be58b34359))
- controller: Use explicit imports and satisfy strict Clippy policy ([13f1336](https://github.com/forkline/robotlb/commit/13f1336e888b28c4364606ddda11b5570ce5c6f1))
- controller: Clean unused imports and move explicit dependencies ([3314c7c](https://github.com/forkline/robotlb/commit/3314c7cd370ae953056adb2558e9926f92693bd4))
- controller: Clean unused imports and move explicit dependencies ([051c0aa](https://github.com/forkline/robotlb/commit/051c0aa0680a4ff010ab4fea6fe99a4a45386983))
- controller: Apply CI rustfmt and kube Resource import ([530783e](https://github.com/forkline/robotlb/commit/530783e8fb55f88bfda7a87550beb10ac6a132db))
- controller: Apply CI rustfmt and kube Resource import ([e006bab](https://github.com/forkline/robotlb/commit/e006bab810e30813c13e62a64029ad411975da6f))
- controller: Align event imports and recorder formatting ([d0c2928](https://github.com/forkline/robotlb/commit/d0c29285573cdde65427a0326d904fa70d7a1c7e))
- controller: Align event imports and recorder formatting ([78475b5](https://github.com/forkline/robotlb/commit/78475b5983cef5978b059fe168c326b24ff6146f))
- controller: Use explicit imports and satisfy strict Clippy policy ([8632343](https://github.com/forkline/robotlb/commit/8632343800d8f6ee5531a079689dfff6d00616a4))
- controller: Use explicit imports and satisfy strict Clippy policy ([888ccaf](https://github.com/forkline/robotlb/commit/888ccaf511c6076dacaea44d29d54918f9100dd7))
- controller: Use explicit imports and satisfy strict Clippy policy ([de1b841](https://github.com/forkline/robotlb/commit/de1b841c68f8f9d9f13cb2faff57d24ed4e79fb0))
- controller: Clean unused imports and move explicit dependencies ([489f69e](https://github.com/forkline/robotlb/commit/489f69eeb19b2ee807362d4ac74f9f43779e4247))
- controller: Clean unused imports and move explicit dependencies ([84b5332](https://github.com/forkline/robotlb/commit/84b5332cd02eecbcdae954bceb4cef24ec6b04f2))
- controller: Apply CI rustfmt and kube Resource import ([0362a22](https://github.com/forkline/robotlb/commit/0362a22149be24ea57c561094e2f4f98bd6166b9))
- controller: Apply CI rustfmt and kube Resource import ([3efb766](https://github.com/forkline/robotlb/commit/3efb76671d8d759604c8149c628b12c2fd620270))
- controller: Apply CI rustfmt and kube Resource import ([06e3409](https://github.com/forkline/robotlb/commit/06e34098386f5663b07b280054d1e176a8cd742c))
- controller: Apply CI rustfmt and kube Resource import ([3034dff](https://github.com/forkline/robotlb/commit/3034dffcb8bebd64029fefedca17f76f6b5c32bd))
- lb: Import cleanup executor and apply cargo fmt to desired-state tests ([ed83ed5](https://github.com/forkline/robotlb/commit/ed83ed50febfe4c0368ac8af0628a568fc61e445))
- lb: Import cleanup executor and apply cargo fmt to desired-state tests ([c171ab5](https://github.com/forkline/robotlb/commit/c171ab531ea090520555ea66e8478ea981cae928))
- lb: Satisfy cargo fmt and pedantic Clippy in controller modules ([bb9c483](https://github.com/forkline/robotlb/commit/bb9c483eaca53f42eeb13176cff39807461f7bac))
- lb: Capacity-safe rolling target replacement and UID-owned cleanup (#70) ([70667ee](https://github.com/forkline/robotlb/commit/70667eeedccb147ee27c3e4b0d84831bb5406cd0))
- tests: Scope finalizer constant import to policy tests ([f267de1](https://github.com/forkline/robotlb/commit/f267de175ea01c2a98058f20c283a075723bb9c0))
- Transparently adopt legacy Hetzner load balancers (#71) ([f4be0fa](https://github.com/forkline/robotlb/commit/f4be0faac3a331bcfb2376212b041c8b9174bca9))

### Refactor

- controller: Add src/controller/reconcile.rs ([490b740](https://github.com/forkline/robotlb/commit/490b740b00321774ced8200db4b2b580875fa8ec))
- controller: Add src/controller/events.rs ([e58dc46](https://github.com/forkline/robotlb/commit/e58dc461621451a66a88fb0f9fd62754c4f61d34))
- controller: Add docs/controller-refactor-plan.md ([550e08f](https://github.com/forkline/robotlb/commit/550e08f026e302c008b21ccfef97a3cc090304c6))
- controller: Extract reconciliation and event responsibilities ([537cb8a](https://github.com/forkline/robotlb/commit/537cb8a6c1f5acec0864dd95e7512d1467b3c6b9))
- controller: Extract reconciliation and event responsibilities ([01c596b](https://github.com/forkline/robotlb/commit/01c596b281b4c86a82c16828c3634226607a5446))
- lb: Resolve typed desired state independently of Hetzner API ([9195b83](https://github.com/forkline/robotlb/commit/9195b8353d28b6ad6b668b6e508d5c61ef8ab82b))
- lb: Add typed, API-independent desired state ([bec3db7](https://github.com/forkline/robotlb/commit/bec3db7c636a81c148c4da0e2870420f35fa1a16))
- lb: Separate typed desired state from Hetzner executor ([b99d117](https://github.com/forkline/robotlb/commit/b99d1173e9536951515e8aa5c800e4c42b754b73))
- lb: Separate typed desired state from Hetzner executor ([71dafd7](https://github.com/forkline/robotlb/commit/71dafd71c045fb822f8781a4044504b4c2c411a8))
- lb: Separate typed desired state from Hetzner executor ([9afb0e1](https://github.com/forkline/robotlb/commit/9afb0e1c2fbfaa8c6f1d9c29a96f45950a74ac1c))
- lb: Separate typed desired state from Hetzner executor ([dd39a8f](https://github.com/forkline/robotlb/commit/dd39a8f9f2c623994bbd2afc64529d5defdf481a))
- lb: Separate typed desired state from Hetzner executor ([4466328](https://github.com/forkline/robotlb/commit/44663284977ff352d733ce2afdf06d7bec619b30))

### Styling

- controller: Match cargo fmt output ([55fcc83](https://github.com/forkline/robotlb/commit/55fcc83af29d9823f1712344af735c20c31f794a))
- controller: Match cargo fmt output ([65abea2](https://github.com/forkline/robotlb/commit/65abea2e1333cb8df1428653c2fbb4a54eef3bee))
- controller: Match cargo fmt output in reconciliation policy tests ([ddf8c56](https://github.com/forkline/robotlb/commit/ddf8c568f456f79fa6cd7dba692efcb0d2500ddf))

### Testing

- controller: Verify lifecycle, event and partial-convergence invariants ([eebf4c4](https://github.com/forkline/robotlb/commit/eebf4c4e8a59c4eb36e755d1476f1eff46397a48))
- controller: Verify lifecycle, event and partial-convergence invariants ([bec65ad](https://github.com/forkline/robotlb/commit/bec65ad585fadbd1e8d347e5abc3f6bfc5348995))
- controller: Verify lifecycle, event and partial-convergence invariants ([96df760](https://github.com/forkline/robotlb/commit/96df760780d79eb3b5872f240fd6d60808688286))
- controller: Avoid moving a shared kube-rs Action ([f436e83](https://github.com/forkline/robotlb/commit/f436e8388768d106ad92d3a40134c19250269062))
- controller: Validate reconciliation policy and partial-provider recovery ([1c2de29](https://github.com/forkline/robotlb/commit/1c2de299f4d5f668dd92fcce23477f8f830f7f49))
- controller: Reconcile policies and recovery with typed desired state ([e38e245](https://github.com/forkline/robotlb/commit/e38e245a43bbd394a641f6f8f977cd273c1bacec))

## [0.7.0](https://github.com/forkline/robotlb/tree/0.7.0) - 2026-10-13

### Added

- Port upstream reconciliation efficiency, failure backoff, multicluster naming and image cleanup ([fc17cbb](https://github.com/forkline/robotlb/commit/fc17cbb))
- Publish Warning Events for rejected LB configurations ([c8051c7](https://github.com/forkline/robotlb/commit/c8051c7))
- Fall back to EndpointSlice-based node discovery for selectorless services ([e33cb1e](https://github.com/forkline/robotlb/commit/e33cb1e))

### Fixed

- Redact Hetzner errors and retry transient balancer deletion ([8531b8f](https://github.com/forkline/robotlb/commit/8531b8f))
- Apply type change before unrelated LB mutations ([e7daa99](https://github.com/forkline/robotlb/commit/e7daa99))
- Use statically typed Service ObjectReference for Events ([ed16ea0](https://github.com/forkline/robotlb/commit/ed16ea0))
- Explicitly construct type list request ([da9f2f1](https://github.com/forkline/robotlb/commit/da9f2f1))
- Grant Event permissions for configuration warnings ([28768bd](https://github.com/forkline/robotlb/commit/28768bd))
- Retain rate limit errors from type lookups and report invalid config ([b181bc2](https://github.com/forkline/robotlb/commit/b181bc2))
- Look up provider capacity with typed error propagation ([fb75517](https://github.com/forkline/robotlb/commit/fb75517))
- Reject capacity mismatches before mutations and require UID ownership ([375f601](https://github.com/forkline/robotlb/commit/375f601))
- Compare typed Kubernetes LoadBalancerIngress status ([e647f38](https://github.com/forkline/robotlb/commit/e647f38))
- Handle zero max_targets in plan_target_actions ([61bc2f5](https://github.com/forkline/robotlb/commit/61bc2f5))
- Retry temporary conflicts for services and networks ([90bb825](https://github.com/forkline/robotlb/commit/90bb825))
- Retry temporary locked/conflicting target and LB operations ([76cc765](https://github.com/forkline/robotlb/commit/76cc765))
- Match Hetzner detach error variant correctly ([bbcdbae](https://github.com/forkline/robotlb/commit/bbcdbae))
- Format ported changes and address current Clippy lint failures ([cfa45dd](https://github.com/forkline/robotlb/commit/cfa45dd))
- Label ownership and safely release Services ([91826aa](https://github.com/forkline/robotlb/commit/91826aa))
- Honor externalTrafficPolicy and selectorless endpoints ([144eefc](https://github.com/forkline/robotlb/commit/144eefc))
- Keep other controllers' finalizers ([9edcdc9](https://github.com/forkline/robotlb/commit/9edcdc9))
- Route load balancer traffic according to externalTrafficPolicy ([7418478](https://github.com/forkline/robotlb/commit/7418478))

### Documentation

- Document UID-only ownership, safe capacity failures and monitoring ([8a6fe41](https://github.com/forkline/robotlb/commit/8a6fe41))
- Document safe upstream migration and test ownership decisions ([9c56de0](https://github.com/forkline/robotlb/commit/9c56de0))

### Performance

- Avoid unchanged LoadBalancer status patches ([a0f0d31](https://github.com/forkline/robotlb/commit/a0f0d31))

### Build

- Refresh Cargo.lock transitives every Monday ([24f0431](https://github.com/forkline/robotlb/commit/24f0431))
- Bump bytes from 1.8.0 to 1.12.1 ([7268ba2](https://github.com/forkline/robotlb/commit/7268ba2))
- Bump tracing-subscriber from 0.3.18 to 0.3.20 ([133882d](https://github.com/forkline/robotlb/commit/133882d))
- Bump rand from 0.8.5 to 0.8.8 ([3e846bf](https://github.com/forkline/robotlb/commit/3e846bf))
- Bump openssl from 0.10.72 to 0.10.81 ([c22ac9b](https://github.com/forkline/robotlb/commit/c22ac9b))
- Bump serde_with from 3.11.0 to 3.24.0 ([f414064](https://github.com/forkline/robotlb/commit/f414064))
- Bump ring from 0.17.8 to 0.17.14 ([3367b72](https://github.com/forkline/robotlb/commit/3367b72))
- Bump tokio from 1.41.1 to 1.43.1 ([ab713b4](https://github.com/forkline/robotlb/commit/ab713b4))
- Bump openssl from 0.10.70 to 0.10.72 ([6b7b182](https://github.com/forkline/robotlb/commit/6b7b182))

## [0.6.0](https://github.com/forkline/robotlb/tree/0.6.0) - 2026-03-29

### Added

- Add Helm unittests for dashboard and prometheus rules ([69a142d](https://github.com/forkline/robotlb/commit/69a142d604e1777382f54570fde534cfa16ef5df))
- Rework dashboard with template variables and improved layout ([baee4af](https://github.com/forkline/robotlb/commit/baee4afb67eaa7517d1479fbe26f2ad3a62c2aa9))
- Support private-only load balancer ingress ([4033706](https://github.com/forkline/robotlb/commit/4033706c9f77ea678d1218355be6ed14a1a86c84))
- Add release skill for opencode ([ed9d63e](https://github.com/forkline/robotlb/commit/ed9d63eec2fd88eeb87c9c169ded0adab2082e50))

### Fixed

- Adjust reconcile duration histogram buckets for millisecond-level precision ([f34a318](https://github.com/forkline/robotlb/commit/f34a3181e8726b3028ac80cc192c7d47996393ce))
- Reduce histogram buckets to essential ranges ([f1f1aed](https://github.com/forkline/robotlb/commit/f1f1aedd1396056f2d0bf21f196fe65aba7bfd41))
- Correct import ordering for cargo fmt ([0cf09b8](https://github.com/forkline/robotlb/commit/0cf09b85ab50596ebd0a50d58621654c4eb0f48e))
- Move dashboard to templates directory ([1ee00e0](https://github.com/forkline/robotlb/commit/1ee00e0e6c6f8ae1ed0e5914aff77a75ab83d001))
- Add trailing newlines to test files ([8c066cf](https://github.com/forkline/robotlb/commit/8c066cf7a19c795d9dc5f894547ee3be3cc584a9))
- Use [\s\S]* in regex to match across newlines in JSON ([64c8720](https://github.com/forkline/robotlb/commit/64c872021cdfa4e5cc23cd9b5c27046e6776a192))
- Resolve pre-commit hook failures ([81293b6](https://github.com/forkline/robotlb/commit/81293b67f2c578ccfa0596e1fde5e1d7c6278add))

### Documentation

- Add documentation for metrics module ([10e9c42](https://github.com/forkline/robotlb/commit/10e9c42c5e7522c91dcb3b420726bac335fc523b))

### Refactor

- Extract shared test utilities to eliminate duplication ([dddd245](https://github.com/forkline/robotlb/commit/dddd245948295a3691b27fcfe0a75345b72c0fa1))
- Standardize annotation constant naming ([5a607f5](https://github.com/forkline/robotlb/commit/5a607f5ae10989b292b8838ff2ae9b4e949730c6))
- Simplify reconcile_network logic ([b2a8b46](https://github.com/forkline/robotlb/commit/b2a8b468f213762c8b4c4eb54094c74b92d87d78))
- Tighten visibility of internal types ([966c0f1](https://github.com/forkline/robotlb/commit/966c0f165df076b417adeb71dd2542a560c0953d))
- Fix visibility and clippy warnings ([146fc26](https://github.com/forkline/robotlb/commit/146fc268f24a9c5d396fa8bcb7dd28a568379b1c))

### Styling

- Add missing newline at end of dashboard.yaml ([09cc4dc](https://github.com/forkline/robotlb/commit/09cc4dcaeb78275d03cc6f17e5f7f79de51d31ea))

## [0.5.0](https://github.com/forkline/robotlb/tree/0.5.0) - 2026-03-04

### Added

- helm: Add Prometheus metrics support ([2e6eb91](https://github.com/forkline/robotlb/commit/2e6eb916b09f9e65e1da67891a42bea3d7b6913f))
- Add Prometheus metrics endpoint ([c69e7ab](https://github.com/forkline/robotlb/commit/c69e7abc4c9b1c44000b0d5a8ef63108ff1afe6c))
- Implement OpenTelemetry distributed tracing integration ([dbcfa8e](https://github.com/forkline/robotlb/commit/dbcfa8eb35bd62da0fadc2332c54ccc17dba3c9a))

### Fixed

- ci: Resolve pre-commit hook failures ([94f6254](https://github.com/forkline/robotlb/commit/94f62541370363fd4b9901eb0dae4991550f690a))
- ci: Correct action field order in auto-tag workflow ([7a4f3d8](https://github.com/forkline/robotlb/commit/7a4f3d8b59a63573ef00d3d382fa85b79f656b6f))
- ci: Downgrade invalid action versions ([77ec84f](https://github.com/forkline/robotlb/commit/77ec84faab22af4a883e3be9b685db010e6640b6))
- Address clippy warnings in metrics module ([1bbe64a](https://github.com/forkline/robotlb/commit/1bbe64a4cc17c6dcc6d3de194e83f80f9964b7bd))
- Resolve clippy warnings in metrics code ([a6f3aef](https://github.com/forkline/robotlb/commit/a6f3aef1f1b38ad6c8916202496dfa34d9d068b8))
- Remove trailing whitespace in docs ([f824f6b](https://github.com/forkline/robotlb/commit/f824f6ba692100370fc53df8d337b590f42edc2d))
- Remove trailing whitespace in docs ([d80b96e](https://github.com/forkline/robotlb/commit/d80b96eaedcc4063c85c79ea85a8c0fe61126160))
- Resolve clippy warnings and formatting issues ([32012da](https://github.com/forkline/robotlb/commit/32012da6daf055f05185b4c9ddca1e781349d7f5))

### Documentation

- Add OpenTelemetry tracing integration trade-off analysis ([b02a625](https://github.com/forkline/robotlb/commit/b02a62511a72912e4ae6292630c55a2b7e3c14e9))

### Refactor

- metrics: Use prometheus crate for metrics ([ae5e42b](https://github.com/forkline/robotlb/commit/ae5e42b49f541667be30c1595957073cb63f7041))
- Modularize codebase and add health checks ([b61c35a](https://github.com/forkline/robotlb/commit/b61c35a38178df79d7c1aefb2f7beea90565a819))
- Use OpenTelemetry for metrics instead of prometheus crate ([9bf163b](https://github.com/forkline/robotlb/commit/9bf163b99f157a8a2ac9672e515235258bbbc08a))

### Helm

- Add OpenTelemetry tracing configuration support ([4876530](https://github.com/forkline/robotlb/commit/48765307cc7c6a54eb196c4dff89e189f287de48))

## [0.4.3](https://github.com/forkline/robotlb/tree/0.4.3) - 2026-02-27

### Added

- Add graceful shutdown handling ([a0b8bed](https://github.com/forkline/robotlb/commit/a0b8bedb9e17eb3f8648b4ded091a36c38605a1d))

### Fixed

- Use correct ipmode field name in service status json ([c0df415](https://github.com/forkline/robotlb/commit/c0df4157007489ce326bbc63d4ed849d198ff28b))

### Release

- Version 0.4.2 ([8fd114f](https://github.com/forkline/robotlb/commit/8fd114fda7e1e22a18259fa4e96f12cdf607b301))

## [0.4.2](https://github.com/forkline/robotlb/tree/0.4.2) - 2026-02-27

### Fixed

- Use correct ipmode field name in service status json ([c0df415](https://github.com/forkline/robotlb/commit/c0df4157007489ce326bbc63d4ed849d198ff28b))

## [0.4.1](https://github.com/forkline/robotlb/tree/0.4.1) - 2026-02-27

### Added

- Set ipMode to Proxy when proxy mode is enabled ([be9cbc3](https://github.com/forkline/robotlb/commit/be9cbc3111a270b7dc20f7960dda5b4846177692))

### Documentation

- Add ha deployment and leader election guidance ([a204747](https://github.com/forkline/robotlb/commit/a204747fe8abeb711c57745656a7327e48b270f4))

## [0.4.0](https://github.com/forkline/robotlb/tree/0.4.0) - 2026-02-25

### Added

- ha: Enable leader election for safe multi-replica runs ([9c71b96](https://github.com/forkline/robotlb/commit/9c71b9635ce3f24ae7a718f3c67a5f1eabf01559))

## [0.3.2](https://github.com/forkline/robotlb/tree/0.3.2) - 2026-02-25

### Fixed

- changelog: Align tag pattern and regenerate changelog ([9ffe8d9](https://github.com/forkline/robotlb/commit/9ffe8d99f53955be11eea68129ff33b45f458f75))

### Chore

- logging: Quiet reconcile debug noise for production ([2255956](https://github.com/forkline/robotlb/commit/22559568b937fa9b66d13d59e1bcddb719a48605))

## [0.3.1](https://github.com/forkline/robotlb/tree/0.3.1) - 2026-02-25

### Fixed

- Trigger reconcile on endpoints updates with resilient fallback ([743159d](https://github.com/forkline/robotlb/commit/743159db222066a7818eea773884666ee186eca9))

## [0.3.0](https://github.com/forkline/robotlb/tree/0.3.0) - 2026-02-25

### Fixed

- deps: Align transitive deps for kube v3 ([3afdc09](https://github.com/forkline/robotlb/commit/3afdc092b8e3da4daca3ecacff8325bfe18b6e9d))
- Satisfy clippy item order in lb tests ([fcfce3d](https://github.com/forkline/robotlb/commit/fcfce3db5703f467087b2cd577cbab504e5faf3b))
- Satisfy clippy send and default-init lints ([d036703](https://github.com/forkline/robotlb/commit/d036703a6162b770e253f6392e4fff05b452da67))
- Resolve clippy lints and format code ([c00007a](https://github.com/forkline/robotlb/commit/c00007a4385082e95e0c99fe324ade843734c0c1))

### Build

- deps: Update Rust crate kube to v3 ([1baef50](https://github.com/forkline/robotlb/commit/1baef506398d601b2b6044f54e9259a891fe0478))
- deps: Update hcloud to v0.25.0 ([004c2f0](https://github.com/forkline/robotlb/commit/004c2f0aebe55677e3f66c4bbeae802a7b0f5c8b))

### Refactor

- Split load balancer reconciliation planning from execution ([47adbbf](https://github.com/forkline/robotlb/commit/47adbbf7af5b86b16689f7551efed085eaf041e8))
- Make finalizer reconciliation idempotent and retry-safe ([4d9ce2b](https://github.com/forkline/robotlb/commit/4d9ce2b850d4a373cadd6fbdacac5832f583aefd))
- Decompose reconcile flow into explicit steps ([7770f48](https://github.com/forkline/robotlb/commit/7770f48a5d23bd827dbba5a518d1f5aca086c59a))

### Styling

- Format lb module with cargo fmt ([5f09e90](https://github.com/forkline/robotlb/commit/5f09e90df7dc8f5207edd95ba09eeed586d4ee04))

### Testing

- Add unit coverage for config and label filters ([9f29b1d](https://github.com/forkline/robotlb/commit/9f29b1d99c1b03242cc9c131d264c2f87172fc48))
- Cover load balancer annotation parsing ([ec9698a](https://github.com/forkline/robotlb/commit/ec9698a8010a5649efa913178259b722670aaec4))
- Add mocked reconcile coverage for lb sync ([b20f389](https://github.com/forkline/robotlb/commit/b20f389435e1e9739d340018f574ba232ed0eea2))
- Cover controller reconcile filtering and derivation ([cafbd0b](https://github.com/forkline/robotlb/commit/cafbd0bc43001595b9c0eddc1c17335cb8e0af77))

## [0.2.1](https://github.com/forkline/robotlb/tree/0.2.1) - 2026-02-25

### Fixed

- ci: Add helm package to create chart ([cda33cf](https://github.com/forkline/robotlb/commit/cda33cf42c1fcca711691ce663184227e4faeeae))

### Build

- deps: Update actions/setup-python action to v6 ([c5b8434](https://github.com/forkline/robotlb/commit/c5b8434d760fa400f8d9efaf69dbf23b06eef053))

## [0.2.0](https://github.com/forkline/robotlb/tree/0.2.0) - 2026-02-25

### Release

- Sync helm chart version with app version ([7000ee5](https://github.com/forkline/robotlb/commit/7000ee56ed7d25159be7192e46a4889e2ee7e964))

## [0.1.1](https://github.com/forkline/robotlb/tree/0.1.1) - 2026-02-25

### Fixed

- chart: Image repository renamed to forkline ([7a32c84](https://github.com/forkline/robotlb/commit/7a32c842ebefc2a81cc7edf5711b74e2e67236d5))
- Resolve all pre-commit CI errors ([8602655](https://github.com/forkline/robotlb/commit/86026559866a0fbbfe24db4ba8d619afa399cb52))

### Build

- deps: Update pre-commit/action action to v3.0.1 ([504dc49](https://github.com/forkline/robotlb/commit/504dc4906b1acfb44773b41bb1c5bcd9690e3b50))
- deps: Update Rust crate clap to v4.5.60 ([42ed67d](https://github.com/forkline/robotlb/commit/42ed67d28efc8f34c8bb39f23b9fafb572e4d642))
- deps: Update Rust crate thiserror to v2.0.18 ([1ce38f1](https://github.com/forkline/robotlb/commit/1ce38f1eee5892c0ea7d871f905c326fe97dbb52))
- deps: Update azure/setup-helm action to v4.3.1 ([12e3d44](https://github.com/forkline/robotlb/commit/12e3d448af9956ad688d4b7629eb9e8f90eac4ea))
- deps: Update Rust crate tokio to v1.49.0 ([b681931](https://github.com/forkline/robotlb/commit/b68193138a667383eef57c30a8fbf5f9428a0986))
- deps: Update docker/login-action action to v3 ([5e3eefa](https://github.com/forkline/robotlb/commit/5e3eefaf722615ff370c3e48cc0627a5de8bbc73))
- deps: Update docker/build-push-action action to v6 ([e1d707e](https://github.com/forkline/robotlb/commit/e1d707e776706e259faeb1aa90aee71b4bb3fe50))
- deps: Update actions/checkout action to v6 ([611c297](https://github.com/forkline/robotlb/commit/611c297e85fb37144844b80b4b6f9b2502bd6f12))
- deps: Update Rust crate tikv-jemallocator to v0.6.1 ([25a057b](https://github.com/forkline/robotlb/commit/25a057b27cfd6934c84c633271bf3a5ea15138d8))
- deps: Update tokio-tracing monorepo ([c9255c5](https://github.com/forkline/robotlb/commit/c9255c5d5c5ac10b2ad08eaf0cd9b4784db55305))
- deps: Update Rust crate futures to v0.3.32 ([f4ab84f](https://github.com/forkline/robotlb/commit/f4ab84fe9baefadd6321b6af45743e15cd5cbf5e))
- renovate: Enable fork processing and onboarding dashboard ([3a57b44](https://github.com/forkline/robotlb/commit/3a57b4472da87fd19d1a7d903aaedd15ab32324e))
- renovate: Remove invalid onboarding repo option ([98696f9](https://github.com/forkline/robotlb/commit/98696f9199ea1b52eb769d311842af1bbc78ebac))

### Release

- Version 0.1.1 ([61246e0](https://github.com/forkline/robotlb/commit/61246e0dea9a12c8871ad388f3a2631e180b01a2))

## [0.1.0](https://github.com/forkline/robotlb/tree/0.1.0) - 2026-02-25

### Build

- Add static checks and devops machinery ([d20ba06](https://github.com/forkline/robotlb/commit/d20ba06b748d9ec636e3e2ef054e49a7d4ccd985))

### Release

- Version 0.1.0 ([6a70265](https://github.com/forkline/robotlb/commit/6a70265af41ac9ac9ce338d0fc8225285dd05b3f))

## [0.0.1](https://github.com/forkline/robotlb/tree/0.0.1) - 2024-11-18

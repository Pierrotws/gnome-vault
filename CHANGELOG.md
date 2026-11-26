# Changelog

## [0.3.1](https://github.com/Pierrotws/passel/compare/v0.3.0...v0.3.1) (2026-11-26)


### Bug Fixes

* **ci:** gettext for the Google Play build, Node 24 actions ([#23](https://github.com/Pierrotws/passel/issues/23)) ([a05b4f5](https://github.com/Pierrotws/passel/commit/a05b4f566a817177e22c44b75d03e10ed8245099))

## [0.3.0](https://github.com/Pierrotws/passel/compare/v0.2.0...v0.3.0) (2026-11-26)


### Features

* **android:** Android app with setup wizard and vaults, two-way sync, logo and Play publishing ([#12](https://github.com/Pierrotws/passel/issues/12)) ([fefd926](https://github.com/Pierrotws/passel/commit/fefd926095b566d15932582b254fef4755757e81))
* **android:** biometric unlock, auto-lock and key backup ([#18](https://github.com/Pierrotws/passel/issues/18)) ([73d65d1](https://github.com/Pierrotws/passel/commit/73d65d11e6595749d4814c14e16bd59b9507bb90))
* **core:** passel-core crate with mobile-ready OpenPGP and git credentials ([#10](https://github.com/Pierrotws/passel/issues/10)) ([a8b671d](https://github.com/Pierrotws/passel/commit/a8b671ddf23da2288ac082936ec01a30b211f7d2))
* first-run wizard and multiple vaults ([#5](https://github.com/Pierrotws/passel/issues/5)) ([ff368e7](https://github.com/Pierrotws/passel/commit/ff368e70ebdf2ea85ac7628cdcbb77726a1fb5ce))
* **i18n:** German, Spanish and Brazilian Portuguese translations ([#14](https://github.com/Pierrotws/passel/issues/14)) ([c136d97](https://github.com/Pierrotws/passel/commit/c136d978d8551ddb0ac5f832f72cc399fe6732df))
* **i18n:** translations with gettext on every target, in English and French ([#13](https://github.com/Pierrotws/passel/issues/13)) ([3b42323](https://github.com/Pierrotws/passel/commit/3b423239cbb21e911267ac869170cee8a861a27c))
* **import:** import entries from KeePassXC and Buttercup vaults ([#8](https://github.com/Pierrotws/passel/issues/8)) ([895e8ba](https://github.com/Pierrotws/passel/commit/895e8bac350514caedf35d5323c0968900a24c8b))
* **windows:** Windows build and installer ([#17](https://github.com/Pierrotws/passel/issues/17)) ([9cc4daa](https://github.com/Pierrotws/passel/commit/9cc4daa2d21e08b9c8ac8341342b551500a9e0ba))


### Bug Fixes

* **android:** sync vaults over https only ([#16](https://github.com/Pierrotws/passel/issues/16)) ([ee14884](https://github.com/Pierrotws/passel/commit/ee14884943836a9e4885f5d520823345738622e1))
* **ui:** sidebar and header review fixes ([#9](https://github.com/Pierrotws/passel/issues/9)) ([9afc59e](https://github.com/Pierrotws/passel/commit/9afc59e7b5d176eb2663e4d9ee3c8fc07cd29020))
* **windows:** start gpg without gpgme's spawn helper ([#21](https://github.com/Pierrotws/passel/issues/21)) ([29c007d](https://github.com/Pierrotws/passel/commit/29c007dec041e7e3f694a853d5b86cd90fb8f18d))

## [0.2.0](https://github.com/Pierrotws/passel/compare/v0.1.1...v0.2.0) (2026-11-24)


### ⚠ BREAKING CHANGES

* the binary is now `passel` and settings live under the new io.pierrotws.Passel schema, so existing preferences reset to their defaults.

### Features

* **entry_preview:** subtitle on first Plain field only ([70fb17e](https://github.com/Pierrotws/passel/commit/70fb17e5483db51f4272b4ca646769cad3745a43))
* **entry_view:** Enter in the title entry triggers save when saveable ([fde96a1](https://github.com/Pierrotws/passel/commit/fde96a1e043bb887e1baa6d4bdf481c1bfadaa0d))
* **entry_view:** make edit mode visually identical to read-only ([d352b6a](https://github.com/Pierrotws/passel/commit/d352b6ad6608e0b09cde849b407ae873698b07b9))
* **entry_view:** put delete before copy on field rows ([b4f6648](https://github.com/Pierrotws/passel/commit/b4f66488c49dc1ea3fa482d093ef1678e41e17f4))
* **group_view:** add right-click "Delete Entry" on entry rows ([0be8829](https://github.com/Pierrotws/passel/commit/0be8829186d7d47c20a9e7236e26e37f9243eef1))
* **otp_field_row:** inset the code and its countdown ([cf3125f](https://github.com/Pierrotws/passel/commit/cf3125f970b74003943a1fc32ed9567ca82601b9))
* **plain_field_row:** size the key column to its longest key ([8c3db50](https://github.com/Pierrotws/passel/commit/8c3db501ad87c17d26b937aac7f9ba3ca6ecd7ed))
* rename the app to Passel ([16eb78b](https://github.com/Pierrotws/passel/commit/16eb78b489ab49fbd47463edb29792d631d17929))
* **store:** rename commit now records old and new entry names ([6ad60f8](https://github.com/Pierrotws/passel/commit/6ad60f8bc872b4e07382b00601ffb51c928d965c))
* **vault_view:** configurable tree-view row size ([5880d9c](https://github.com/Pierrotws/passel/commit/5880d9c70d6c2eb20474819c6a0ee52f16c88547))
* **vault_view:** delete folder action in tree right-click menu ([dfaa618](https://github.com/Pierrotws/passel/commit/dfaa618506d673ff38569e21189aa1cce9837c70))
* **vault_view:** drop the tree's view background ([2ea60d4](https://github.com/Pierrotws/passel/commit/2ea60d40f18601e4988d3f03fbb39be7de443ef1))
* **vault_view:** scale tree-view label font 1.2x in Large mode ([f26a8d7](https://github.com/Pierrotws/passel/commit/f26a8d75f70a1a7763c7231981dfe5568417675c))
* **window:** badge unpushed-commit count on the Changes button ([124fcf4](https://github.com/Pierrotws/passel/commit/124fcf4028d8de2f1e202052a29121467251cc04))
* **window:** confirm entry deletion in both delete paths ([cff3884](https://github.com/Pierrotws/passel/commit/cff3884a5ebcf70c28823a40c7ff8d5bd1403499))
* **window:** give every column a minimum width ([0a3dba0](https://github.com/Pierrotws/passel/commit/0a3dba0af2973215d8072626ac5bf0cfcfe7a47f))
* **window:** open narrower, with a third for the columns ([58361f6](https://github.com/Pierrotws/passel/commit/58361f606687dd2e7142f58149171bcbb3418f76))
* **window:** open the tree column narrower ([1a3dfca](https://github.com/Pierrotws/passel/commit/1a3dfcad7ee6dc860c913d7b72c521e52fdf6466))


### Bug Fixes

* **a11y:** explicit accessible-name on every icon-only field-row button ([1b5bd38](https://github.com/Pierrotws/passel/commit/1b5bd38bcc7235055a1ce9546b024e9bd8b9103f))
* **autoload:** cancel in-flight worker before starting a new one ([3ff676e](https://github.com/Pierrotws/passel/commit/3ff676e059c9093ff24b79cc54a3d65957e91f11))
* **autoload:** cancellable autoload worker via shared atomic flag ([4b0ff5f](https://github.com/Pierrotws/passel/commit/4b0ff5f4a39ad178fb5c67d7493f9f1e493ae51e))
* **changes_view:** pass has_more explicitly to ChangesView; probe one extra commit ([d8c64d2](https://github.com/Pierrotws/passel/commit/d8c64d23652e7e035df4c832a0c2598e8de40ee9))
* **controller:** gate setup_vault cache flush on the new tree being non-empty ([316e59c](https://github.com/Pierrotws/passel/commit/316e59c8b48ee4b9d3b69ff499b3b45c7c23178a))
* **controller:** kind-check delete_* + skip secret values in entry_matches_query ([3827685](https://github.com/Pierrotws/passel/commit/3827685b830124958df8bd3dda4bdc6a695cd2b9))
* **controller:** scope delete_folder cache eviction to the deleted prefix ([6c2f41f](https://github.com/Pierrotws/passel/commit/6c2f41fe58971bb539c7f67913ee849145dfa235))
* **controller:** surface RestartRequired when setup wizard collides with pinned env ([380cd60](https://github.com/Pierrotws/passel/commit/380cd60f0950af21eec3d47ae56d6ebd714588b5))
* **controller:** trim branch on store so the getter returns the canonical form ([0a1cb91](https://github.com/Pierrotws/passel/commit/0a1cb911425c7bcf5eb0cb70a49299f2bd643222))
* **entry_view:** cap the title size ([770cec6](https://github.com/Pierrotws/passel/commit/770cec67e8290f545fa3c1d7819f25c26de289f8))
* **entry_view:** drag-and-drop carries the row GObject, not its captured index ([0b9fe4a](https://github.com/Pierrotws/passel/commit/0b9fe4a8f6e015457680de7dbc6da9836e39fa96))
* **errors:** hand-roll Debug for StoreError + PgpError so it matches Display redaction ([4e10af0](https://github.com/Pierrotws/passel/commit/4e10af07825160a5bb2a537526795254c12fe711))
* **errors:** redact paths and external stderr from user-facing Display ([50745f8](https://github.com/Pierrotws/passel/commit/50745f87258e847f44e1d02c12db5c98b0f074a4))
* **errors:** redact upstream messages in PgpError and GitError Display ([2d43a16](https://github.com/Pierrotws/passel/commit/2d43a160576a6ec80ef1f9002016e22335b47c20))
* **generate_password_view:** weak-capture self in setup closures ([c602279](https://github.com/Pierrotws/passel/commit/c602279d825d2a1a3bea06ad18ce051839d02ee5))
* **git:** append random suffix to rollback backup branch name ([8a49918](https://github.com/Pierrotws/passel/commit/8a499183e79218c006cb9941f2b80dd48f08c131))
* **git:** cleanup_state on revert_commit conflict early-return ([a5dd691](https://github.com/Pierrotws/passel/commit/a5dd69180c3ac2b2dd1211e0625a5ab78ac665f7))
* **git:** log when the rollback backup-ref delete itself fails ([d50cfb4](https://github.com/Pierrotws/passel/commit/d50cfb471c6ac3ffbde627c9c1b09f123e22ede5))
* **git:** surface a typed NoSignature error when user.name/email are unset ([ec51a3d](https://github.com/Pierrotws/passel/commit/ec51a3dfd11876ac567cecdf6c2ea241b703eb6c))
* **git:** thread branch override through unpushed_count ([6414f67](https://github.com/Pierrotws/passel/commit/6414f677676676c80bb426deeb726caa7e1bee19))
* **group_content:** snapshot subtitles before driving items_changed setters ([bdd4708](https://github.com/Pierrotws/passel/commit/bdd4708640f120a8d33637d0f15599f0068f791a))
* **group_preview:** thread an Arc&lt;AtomicBool&gt; cancel through the worker ([444951d](https://github.com/Pierrotws/passel/commit/444951d5c2b9539ec55f53c8da3387b9eab7f6a0))
* **group_view:** entry-activated emits the node path, not its row index ([6e9472a](https://github.com/Pierrotws/passel/commit/6e9472a2bf50d79ecc8506b75099cb2c4c8dfaa9))
* **group_view:** give entry rows their final height before previews land ([c601c02](https://github.com/Pierrotws/passel/commit/c601c02ca1a7d6145c357880751b7641215683b5))
* **group_view:** splice a fresh GroupItem to actually trigger row rebind ([3e2a802](https://github.com/Pierrotws/passel/commit/3e2a8027ccde2107949a3947bbc39356680c6b36))
* **main:** register GeneratePasswordView GType once at startup ([7c19275](https://github.com/Pierrotws/passel/commit/7c1927526a27f9e8c62d791f24cf3130802c83a8))
* **multiline_field_row:** skip sync_value_view rebuild while editing ([ca4bc73](https://github.com/Pierrotws/passel/commit/ca4bc7348eda8e61af8dbc67e91e35b490c35afb))
* **new_entry_dialog:** pre-validate folder, trim entry name on submit ([21fbc5f](https://github.com/Pierrotws/passel/commit/21fbc5fa9542724d9552d50683c14bad0d1bcfa0))
* **otp_field_row:** apply PRIVATE/NO_EMOJI/NO_SPELLCHECK hints to url_entry ([b06e67c](https://github.com/Pierrotws/passel/commit/b06e67cb0b303b32c0edc551fcf06211cf73510c))
* **otp_field_row:** mask the URL as blank rather than a run of dots ([a32f63a](https://github.com/Pierrotws/passel/commit/a32f63aff88d5c1c995e2a7736f5101ce378b83c))
* **otp:** percent_decode now handles %XX escapes flush against end of value ([3ca5302](https://github.com/Pierrotws/passel/commit/3ca5302ea313b66144151bb38ebf95be9aab615e))
* **otp:** surface MalformedUrl on invalid UTF-8 percent-decode ([eac57b9](https://github.com/Pierrotws/passel/commit/eac57b93aeffdfaf7845bb28e20c88c906b2280e))
* **parser:** stop allocating block-scalar lines into raw heap on parse ([b8955ee](https://github.com/Pierrotws/passel/commit/b8955eee3c9707716fcd328e5c4a9576b67ecec7))
* **parser:** strip leading UTF-8 BOM before parsing entry plaintext ([48a71b6](https://github.com/Pierrotws/passel/commit/48a71b6f09708942a059f59d6555d7e4ee15a297))
* **parser:** wrap format_entry output in Zeroizing&lt;String&gt; ([51ef696](https://github.com/Pierrotws/passel/commit/51ef696b158bc81ebcc3e4eb89181afb8ab7656d))
* **password_field_row:** guard set_entry_field on the Password variant ([d856f5f](https://github.com/Pierrotws/passel/commit/d856f5ff607ca1a9995bb5f4ca928933d8b0497c))
* **password_field_row:** privacy hints on the password entry to keep IMs from logging it ([6f334a8](https://github.com/Pierrotws/passel/commit/6f334a80785b1ae02cddd6d35ac8fb6b1d8e077d))
* **password:** generate_password returns Result instead of panicking ([794c0bb](https://github.com/Pierrotws/passel/commit/794c0bb4116ee29273b01311df469806e10dd476))
* **password:** zeroize the password generator path end-to-end ([9524a71](https://github.com/Pierrotws/passel/commit/9524a710f82f2423348fad08a5f730fb2d6000ba))
* **pgp:** re-validate recipient keys on encrypt + reject out-of-store entry_parent ([c1f4ecf](https://github.com/Pierrotws/passel/commit/c1f4ecfcd940da2bd8424b1dd2f19d912e989e06))
* **pgp:** refuse to encrypt with an empty recipient list ([d263879](https://github.com/Pierrotws/passel/commit/d26387991c01a40250f07e23a612226169e25692))
* **pgp:** sanitize GPG user-id strings before showing them as labels ([c05cb5a](https://github.com/Pierrotws/passel/commit/c05cb5af4b9d490ac98b143f7c20b062b72292f7))
* **pgp:** validate UTF-8 in place so the failure path stays Zeroizing ([dbfd799](https://github.com/Pierrotws/passel/commit/dbfd7992110b5c82af517bbc04d28f0edff7bb5c))
* **preferences:** log unknown size value ([bbe9973](https://github.com/Pierrotws/passel/commit/bbe9973c1faf1288dc77c4369ce58b0c7402562c))
* **preferences:** mirror gschema defaults in setting_boolean fallbacks ([610768a](https://github.com/Pierrotws/passel/commit/610768af9cc941ca71996c71a18e84716bb365f8))
* **state:** redact cache hit/miss log + guard evict_cached_entries_under ([6686e38](https://github.com/Pierrotws/passel/commit/6686e38b14fd983831363566de9ac15cd573dc60))
* **state:** redact entry name from EntrySession Debug in non-test builds ([bd1a087](https://github.com/Pierrotws/passel/commit/bd1a0878d441a8f6455804b5cd50034f0b2d22d1))
* **state:** snap candidate name to node.name in mark_saved ([edb3a24](https://github.com/Pierrotws/passel/commit/edb3a24ee2355ac2842fef0a61bb65d53f54d245))
* **store:** accept short/long key-id when validating setup_vault recipient ([90a237f](https://github.com/Pierrotws/passel/commit/90a237fae093592bce705bcf5a412a8ea5abdb60))
* **store:** don't panic password_store_dir when HOME is unset ([f44925a](https://github.com/Pierrotws/passel/commit/f44925ae6036b8be44f0694a2572979d30e8c995))
* **store:** fsync parent directory after atomic rename ([437b33c](https://github.com/Pierrotws/passel/commit/437b33c2051f8fdc49e1a8d02e12e66d001aa2ce))
* **store:** gate PASSWORD_STORE_DIR env::set_var via OnceLock ([0d17346](https://github.com/Pierrotws/passel/commit/0d17346d48d9ea4ac9d8e7840bc7b108bbf0dfbe))
* **store:** re-stage the index when restore_previous puts bytes back ([adc22be](https://github.com/Pierrotws/passel/commit/adc22bed56a2b84234b6345892ae706a08a79625))
* **store:** reject newlines in entry names ([df93920](https://github.com/Pierrotws/passel/commit/df939209625033b06bf96f03f90d278b206c41db))
* **store:** setup_vault refuses to overwrite an existing .gpg-id ([42e9fe3](https://github.com/Pierrotws/passel/commit/42e9fe3adbb28809fa8c754cd9d172e9e419439b))
* **store:** skip non-UTF8 paths during load_dir instead of mangling them ([ab22575](https://github.com/Pierrotws/passel/commit/ab22575fb6124df407048ba6b165a1f46040fdac))
* **store:** verify setup_vault recipient against available local keys ([debf54b](https://github.com/Pierrotws/passel/commit/debf54b51b52372083ba01357aeb7eb45614ab46))
* **vault_view:** unparent the row context-menu popover when it closes ([9f3dfa4](https://github.com/Pierrotws/passel/commit/9f3dfa47f70078b66ed0d570fa85190d91f3f002))
* **window:** block paned position_notify across the corrective set_position ([1bc7ad8](https://github.com/Pierrotws/passel/commit/1bc7ad8db04c1bcfd2725a67cda086323714d6b7))
* **window:** keep the tree's width when columns are resized ([553771c](https://github.com/Pierrotws/passel/commit/553771cab9467f0ea89405f1cba18c55042f5c84))
* **window:** re-warm autoload after revert and rollback ([b4a4ded](https://github.com/Pierrotws/passel/commit/b4a4dedbbe8f2ce88ab3b896d2e88508e1daae7f))
* **window:** read live is_dirty in the lock-button toggle ([f09f70f](https://github.com/Pierrotws/passel/commit/f09f70f299897b55df23a0b45d6c8da6e210e9fd))
* **window:** refuse to switch entries while the current one has unsaved edits ([35fb104](https://github.com/Pierrotws/passel/commit/35fb104886e5a660fe8958449efcd20e10ad66f2))
* **window:** scrub remaining "Push"/"Rollback" wording from user-facing strings ([eb09c3f](https://github.com/Pierrotws/passel/commit/eb09c3fb776f4adffa30c3118b0e93dc654833f8))
* **window:** skip vault-tree parent selection when activated entry is top-level ([fc0438a](https://github.com/Pierrotws/passel/commit/fc0438a0fdf356e8a5a6f972a7c0fdd2d3e6c44e))
* **window:** weak-capture self in setup_callbacks and timer closures ([2861fae](https://github.com/Pierrotws/passel/commit/2861faebb76ae039a812ae6eb2b6e64588883230))
* **workers:** drop entry paths from cache_warmup / group_preview Failed payloads ([f3439a9](https://github.com/Pierrotws/passel/commit/f3439a90b0c452ff64473a932cda76fff5214e51))


### Performance Improvements

* **changes_view:** drop per-row Cell::take and set_data thrash ([592f278](https://github.com/Pierrotws/passel/commit/592f2784072dfc7f916a9bbb79a60e2e55dd2b8f))
* **controller:** drop per-keystroke allocation in search filter ([d6ffa0b](https://github.com/Pierrotws/passel/commit/d6ffa0b28d4a9d68ce89c0d2a8772624549cdd15))
* **entry_view:** stop syncing title_label per keystroke ([23f150f](https://github.com/Pierrotws/passel/commit/23f150f16be8ad45a6ab4d098d826747e6d8fe27))
* **window:** drop per-keystroke update_selection_layout from rebuild_vault_tree ([e2a09a2](https://github.com/Pierrotws/passel/commit/e2a09a23130475fb3bb67d36178c774ecdc9da4e))

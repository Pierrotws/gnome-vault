# Website

Passel's website, <https://pierrotws.github.io/passel/>, built with
Jekyll and published on GitHub Pages by
[`.github/workflows/pages.yml`](../.github/workflows/pages.yml) on every
change to `main`. Pull requests only build it.

Some of its content lives elsewhere and is copied in by the workflow,
so there is a single source for each:

| On the site | Source |
| --- | --- |
| `/privacy/` | [`PRIVACY.md`](../PRIVACY.md) — Google Play links to this address: keep it |
| `assets/logo.svg` | [`assets/logo.svg`](../assets/logo.svg) |
| `assets/feature-graphic.png` | [`assets/android/store/`](../assets/android/store) |
| `assets/screenshots/android/` | [`assets/android/store/phone/`](../assets/android/store/phone) |

`assets/screenshots/desktop-*.png` are the desktop app on demo vaults
(1280×800, English), taken under Xvfb.

To preview it, copy the site together like the workflow's "Site source"
step does, then build it with the same image as GitHub Pages:

```bash
docker run --rm -v "$PWD/build/web:/github/workspace" \
  -e GITHUB_WORKSPACE=/github/workspace -e INPUT_SOURCE=site \
  -e INPUT_DESTINATION=_site -e GITHUB_REPOSITORY=Pierrotws/passel \
  ghcr.io/actions/jekyll-build-pages:v1.0.13
```

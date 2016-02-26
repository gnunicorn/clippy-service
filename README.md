# Clippy

[![](https://img.shields.io/badge/Code%20Style-rustfmt-brightgreen.svg?style=flat-square)](https://github.com/rust-lang-nursery/rustfmt#configuring-rustfmt) ![](http://clippy.bashy.io/github/ligthyear/clippy-service/master/badge.svg)]

## Documentation

Clippy has inline source code annotations and uses "docco" to render those into webpages. To update the rendered HTML to the latest version please run docco  from the repos root as follows:

```
docco --languages docco-langs.json -o static/docs src/*.rs
```

Clippy automatically picks it up and hosts the latest version found in static/docs.

## License: AGPL 3.0

This source code, the repository and all documentation is released under the GNU Affero General Public License 3.0. To gain a rough understanding what that means for you, please take a look at [tl;drLegal](https://tldrlegal.com/license/gnu-affero-general-public-license-v3-%28agpl-3.0%29#summary), however only the text written in the shipped LICENSE file is legally binding. If you have any questions about the license and whether your planned use of it may be conflicting, please consult the bashy.io team via github.

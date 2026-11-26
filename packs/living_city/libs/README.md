# Living City: libraries

The libraries only Living City uses, stored byte for byte as published:
the local AI (llama.cpp through `llama-cpp-2` and `llama-cpp-sys-2`), the
tools that build llama.cpp (`bindgen`, `cmake`, `clang-sys` and their
helpers) and `regex`. The top-level Cargo.toml points the build here with
`[patch.crates-io]`. The libraries Living City shares with the game are in
the top-level libs/ folder.

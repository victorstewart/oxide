# C07 Metal neon-marker ABI evidence

This directory records the C07 correctness and performance evidence for making the Metal neon-marker instance ABI explicit and portable.

The parent uploads a 68-byte Rust instance to shaders whose naturally aligned `float4` fields require an 80-byte instance, so Metal validation aborts even at one marker and array strides cannot agree. The selected ABI uses 8-byte Rust alignment, packed Metal color vectors, and an explicit shared tail word for a 72-byte stride. C07 keeps inline marker payloads but splits them into legal 4 KiB chunks, making all 128 markers correct without taking the uniform-ring optimization owned by C09.

Compile-time Rust size/alignment/offset assertions, shader-source contracts, and byte-exact Metal snapshots cover 1, 2, 51, 52, 60, 61, and 128 markers. The locally ignored `raw/` directory records the parent validation abort, the packed-versus-aligned selection, and the parent-versus-final release results. Official `latest.*` baselines remain unchanged until C62.

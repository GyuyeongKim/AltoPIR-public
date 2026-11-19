fn main() {
    // Compile C++ matmul (AVX kernels for CPU fallback)
    println!("cargo:rerun-if-changed=src/matmul.cpp");
    cc::Build::new()
        .cpp(true)
        .file("src/matmul.cpp")
        .flag("-O3")
        .flag("-march=native")
        .flag("-std=c++11")
        .compile("matmul");

    println!("cargo:rerun-if-changed=src/amx_kernels.cpp");
    cc::Build::new()
        .cpp(true)
        .file("src/amx_kernels.cpp")
        .flag("-O3")
        .flag("-march=native")
        .flag("-mamx-tile")
        .flag("-mamx-int8")
        .flag("-fopenmp")
        .flag("-std=c++17")
        .compile("amx_kernels");
    println!("cargo:rustc-link-lib=gomp");
}

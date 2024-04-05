fn main() {
    // Without statically linked VCRuntime the app will require Visual C++ Redistributable installed on host OS.
    static_vcruntime::metabuild();
}

// O cargo não sabe que os programas dependem do script de ligação: sem esta
// linha, mudar o linker.ld não faria nada até o próximo `cargo clean`.
fn main() {
    println!("cargo:rerun-if-changed=linker.ld");
}

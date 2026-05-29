use vergen_git2::{Emitter, Git2};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let git = Git2::builder().branch(true).dirty(true).sha(true).build();
    Emitter::default().add_instructions(&git)?.emit()?;
    Ok(())
}

use crate::global::states::{HashMode, KeyFile, PasswordMode, SkipMode};

use super::states::{DirectoryMode, EraseMode, HiddenFilesMode, PrintMode, DeleteSourceDir};

pub struct CryptoParams {
    pub hash_mode: HashMode,
    pub skip: SkipMode,
    pub password: PasswordMode,
    pub erase: EraseMode,
    pub keyfile: KeyFile,
}

pub struct PackParams {
    pub dir_mode: DirectoryMode,
    pub hidden: HiddenFilesMode,
    pub exclude: Vec<String>,
    pub print_mode: PrintMode,
    pub delete_source: DeleteSourceDir,
}

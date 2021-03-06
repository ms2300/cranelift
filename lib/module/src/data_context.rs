//! Defines `DataContext`.

use cranelift_codegen::binemit::{Addend, CodeOffset};
use cranelift_codegen::entity::PrimaryMap;
use cranelift_codegen::ir;
use std::boxed::Box;
use std::vec::Vec;

/// This specifies how data is to be initialized.
#[derive(PartialEq, Eq, Debug)]
pub enum Init {
    /// This indicates that no initialization has been specified yet.
    Uninitialized,
    /// Initialize the data with all zeros.
    Zeros {
        /// The size of the data.
        size: usize,
    },
    /// Initialize the data with the specified contents.
    Bytes {
        /// The contents, which also implies the size of the data.
        contents: Box<[u8]>,
    },
}

impl Init {
    /// Return the size of the data to be initialized.
    pub fn size(&self) -> usize {
        match *self {
            Init::Uninitialized => panic!("data size not initialized yet"),
            Init::Zeros { size } => size,
            Init::Bytes { ref contents } => contents.len(),
        }
    }
}

/// A flag specifying whether data is readonly or may be written to.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Writability {
    /// Data is readonly, meaning writes to it will trap.
    Readonly,
    /// Data is writable.
    Writable,
}

/// A description of a data object.
pub struct DataDescription {
    /// Whether the data readonly or writable.
    pub writable: Writability,
    /// How the data should be initialized.
    pub init: Init,
    /// External function declarations.
    pub function_decls: PrimaryMap<ir::FuncRef, ir::ExternalName>,
    /// External data object declarations.
    pub data_decls: PrimaryMap<ir::GlobalValue, ir::ExternalName>,
    /// Function addresses to write at specified offsets.
    pub function_relocs: Vec<(CodeOffset, ir::FuncRef)>,
    /// Data addresses to write at specified offsets.
    pub data_relocs: Vec<(CodeOffset, ir::GlobalValue, Addend)>,
}

/// This is to data objects what cranelift_codegen::Context is to functions.
pub struct DataContext {
    description: DataDescription,
}

impl DataContext {
    /// Allocate a new context.
    pub fn new() -> Self {
        Self {
            description: DataDescription {
                writable: Writability::Readonly,
                init: Init::Uninitialized,
                function_decls: PrimaryMap::new(),
                data_decls: PrimaryMap::new(),
                function_relocs: Vec::new(),
                data_relocs: Vec::new(),
            },
        }
    }

    /// Clear all data structures in this context.
    pub fn clear(&mut self) {
        self.description.writable = Writability::Readonly;
        self.description.init = Init::Uninitialized;
        self.description.function_decls.clear();
        self.description.data_decls.clear();
        self.description.function_relocs.clear();
        self.description.data_relocs.clear();
    }

    /// Define a zero-initialized object with the given size.
    pub fn define_zeroinit(&mut self, size: usize, writable: Writability) {
        debug_assert_eq!(self.description.init, Init::Uninitialized);
        self.description.writable = writable;
        self.description.init = Init::Zeros { size };
    }

    /// Define an object initialized with the given contents.
    ///
    /// TODO: Can we avoid a Box here?
    pub fn define(&mut self, contents: Box<[u8]>, writable: Writability) {
        debug_assert_eq!(self.description.init, Init::Uninitialized);
        self.description.writable = writable;
        self.description.init = Init::Bytes { contents };
    }

    /// Declare an external function import.
    ///
    /// Users of the `Module` API generally should call
    /// `Module::declare_func_in_data` instead, as it takes care of generating
    /// the appropriate `ExternalName`.
    pub fn import_function(&mut self, name: ir::ExternalName) -> ir::FuncRef {
        self.description.function_decls.push(name)
    }

    /// Declares a global value import.
    ///
    /// TODO: Rename to import_data?
    ///
    /// Users of the `Module` API generally should call
    /// `Module::declare_data_in_data` instead, as it takes care of generating
    /// the appropriate `ExternalName`.
    pub fn import_global_value(&mut self, name: ir::ExternalName) -> ir::GlobalValue {
        self.description.data_decls.push(name)
    }

    /// Write the address of `func` into the data at offset `offset`.
    pub fn write_function_addr(&mut self, offset: CodeOffset, func: ir::FuncRef) {
        self.description.function_relocs.push((offset, func))
    }

    /// Write the address of `data` into the data at offset `offset`.
    pub fn write_data_addr(&mut self, offset: CodeOffset, data: ir::GlobalValue, addend: Addend) {
        self.description.data_relocs.push((offset, data, addend))
    }

    /// Reference the initializer data.
    pub fn description(&self) -> &DataDescription {
        debug_assert!(
            self.description.init != Init::Uninitialized,
            "data must be initialized first"
        );
        &self.description
    }
}

#[cfg(test)]
mod tests {
    use cranelift_codegen::ir;
    use {DataContext, Init, Writability};

    #[test]
    fn basic_data_context() {
        let mut data_ctx = DataContext::new();
        {
            let description = &data_ctx.description;
            assert_eq!(description.writable, Writability::Readonly);
            assert_eq!(description.init, Init::Uninitialized);
            assert!(description.function_decls.is_empty());
            assert!(description.data_decls.is_empty());
            assert!(description.function_relocs.is_empty());
            assert!(description.data_relocs.is_empty());
        }

        data_ctx.define_zeroinit(256, Writability::Writable);

        let _func_a = data_ctx.import_function(ir::ExternalName::user(0, 0));
        let func_b = data_ctx.import_function(ir::ExternalName::user(0, 1));
        let func_c = data_ctx.import_function(ir::ExternalName::user(1, 0));
        let _data_a = data_ctx.import_global_value(ir::ExternalName::user(2, 2));
        let data_b = data_ctx.import_global_value(ir::ExternalName::user(2, 3));

        data_ctx.write_function_addr(8, func_b);
        data_ctx.write_function_addr(16, func_c);
        data_ctx.write_data_addr(32, data_b, 27);

        {
            let description = data_ctx.description();
            assert_eq!(description.writable, Writability::Writable);
            assert_eq!(description.init, Init::Zeros { size: 256 });
            assert_eq!(description.function_decls.len(), 3);
            assert_eq!(description.data_decls.len(), 2);
            assert_eq!(description.function_relocs.len(), 2);
            assert_eq!(description.data_relocs.len(), 1);
        }

        data_ctx.clear();
        {
            let description = &data_ctx.description;
            assert_eq!(description.writable, Writability::Readonly);
            assert_eq!(description.init, Init::Uninitialized);
            assert!(description.function_decls.is_empty());
            assert!(description.data_decls.is_empty());
            assert!(description.function_relocs.is_empty());
            assert!(description.data_relocs.is_empty());
        }

        let contents = vec![33, 34, 35, 36];
        let contents_clone = contents.clone();
        data_ctx.define(contents.into_boxed_slice(), Writability::Readonly);
        {
            let description = data_ctx.description();
            assert_eq!(description.writable, Writability::Readonly);
            assert_eq!(
                description.init,
                Init::Bytes {
                    contents: contents_clone.into_boxed_slice()
                }
            );
            assert_eq!(description.function_decls.len(), 0);
            assert_eq!(description.data_decls.len(), 0);
            assert_eq!(description.function_relocs.len(), 0);
            assert_eq!(description.data_relocs.len(), 0);
        }
    }
}

(module
  (type (;0;) (func (param i32 i32) (result i32)))
  (export "nand" (func 0))
  (func (;0;) (type 0) (param i32 i32) (result i32)
    local.get 0
    local.get 1
    i32.and
    i32.const 1
    i32.xor
  )
)

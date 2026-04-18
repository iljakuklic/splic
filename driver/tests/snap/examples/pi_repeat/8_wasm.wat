(module
  (type (;0;) (func (param i64) (result i64)))
  (export "square_twice" (func 0))
  (func (;0;) (type 0) (param i64) (result i64)
    local.get 0
    local.get 0
    i64.mul
    local.get 0
    local.get 0
    i64.mul
    i64.mul
  )
)

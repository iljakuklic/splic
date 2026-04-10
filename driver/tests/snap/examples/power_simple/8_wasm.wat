(module
  (type (;0;) (func (param i64) (result i64)))
  (type (;1;) (func (param i64) (result i64)))
  (export "pow5" (func 0))
  (export "pow11" (func 1))
  (func (;0;) (type 0) (param i64) (result i64)
    local.get 0
    local.get 0
    i64.mul
    local.get 0
    i64.mul
    local.get 0
    i64.mul
    local.get 0
    i64.mul
  )
  (func (;1;) (type 1) (param i64) (result i64)
    local.get 0
    local.get 0
    i64.mul
    local.get 0
    i64.mul
    local.get 0
    i64.mul
    local.get 0
    i64.mul
    local.get 0
    i64.mul
    local.get 0
    i64.mul
    local.get 0
    i64.mul
    local.get 0
    i64.mul
    local.get 0
    i64.mul
    local.get 0
    i64.mul
  )
)

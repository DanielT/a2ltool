// update_test.elf built with: arm-none-eabi-gcc, Arm GNU Toolchain 13.2
// arm-none-eabi-gcc -mcpu=cortex-m7 -mthumb -specs=nano.specs -mfloat-abi=hard -nostdlib -g3 update_test.c -o update_test.elf

#include "stdint.h"

typedef enum {
    VALUE_1 = 100,
    VALUE_2 = 20,
    VALUE_3 = 11111
} MyEnum;

typedef uint32_t (*funcptr_t)(uint16_t*, float);

typedef struct StructA {
    MyEnum enumval;
    uint32_t val_i32;
    uint64_t val_i64;
    float val_f32;
} StructA;

struct StructB {
    struct StructB* pPrev;
    struct StructB* pNext;
    StructA s1;
    StructA s2;
    funcptr_t func;
};

typedef struct {

} BasicData;

typedef struct RegDef {
    union {
        uint32_t Value;
        struct {
            unsigned Bits_ABC:5;
            unsigned Bits_DEF:5;
            unsigned Bits_GHI:5;
            unsigned Bits_JKL:5;
            unsigned :12;
        };
    };
} RegDef;

typedef struct {
    uint32_t value;
} TestStruct;

uint8_t  val_u8;
uint16_t val_u16;
uint32_t val_u32;
uint64_t val_u64;
int8_t   val_i8;
int16_t  val_i16;
int32_t  val_i32;
int64_t  val_i64;
float    val_f;
double   val_d;
MyEnum   val_e;
void*    val_ptr;

RegDef reg;
struct StructB struct_b;
BasicData basic;
funcptr_t func;

TestStruct    TEST_struct = {0};
TestStruct   *TEST_structptr = &TEST_struct;
TestStruct  **TEST_structptr_ptr = &TEST_structptr;
TestStruct    TEST_structarr[10] = { {0}, {0}, {0}, {0}, {0}, {0}, {0}, {0}, {0}, {0} };
TestStruct    TEST_structarr_arr[2][10] = {{ {0}, {0}, {0}, {0}, {0}, {0}, {0}, {0}, {0}, {0} }, { {0}, {0}, {0}, {0}, {0}, {0}, {0}, {0}, {0}, {0} }};
TestStruct  (*TEST_structarr_ptr)[10] = &TEST_structarr;
TestStruct  (*TEST_structarr_ptr_arr[2])[10] = {&TEST_structarr, &TEST_structarr};
TestStruct   *TEST_structptr_arr[4] = { &TEST_struct, &TEST_struct, &TEST_struct, &TEST_struct};
TestStruct *(*TEST_structptr_arr_ptr)[4] = &TEST_structptr_arr;

uint32_t Value_u32;
int8_t Value_i8;

int main()
{
    return 0;
}
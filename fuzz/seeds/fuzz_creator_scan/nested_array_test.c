// nested_array_test.elf built with: clang 22
// clang --target=armv7m-none-eabi -mcpu=cortex-m7 -mfloat-abi=hard -nostdlib -fuse-ld=lld -g3 nested_array_test.c -o nested_array_test.elf
//
// This test case exists because clang represents the type of "pts" as nested array types:
//     Array(dim 10) -> typedef Vec3 -> Array(dim 3) -> float
// while "plain" gets a single array type with two dimensions.
// gcc (13/15) flattens both declarations into a single array type with two dimensions,
// so this input cannot be created with gcc.
//
// Both members of struct S describe the identical memory layout (30 floats), so both
// representations must result in the same information in the a2l file.

typedef float Vec3[3];

struct S {
    Vec3 pts[10];
    float plain[10][3];
};

struct S s;
Vec3 varr[10];

void _start(void) {
    s.pts[0][0] = 1.0f;
    s.plain[0][0] = 2.0f;
    varr[0][0] = 3.0f;
}

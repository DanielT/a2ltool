// update_test.elf built with: arm-none-eabi-gcc, Arm GNU Toolchain 13.2
// arm-none-eabi-gcc -mcpu=cortex-m7 -mthumb -specs=nano.specs -mfloat-abi=hard -nostdlib -g3 update_test.c -o update_test.elf

#include <stdint.h>

/**********************************************
 * curve with an internal axis                */
struct UpdateTest_Curve_InternalAxis {
    uint16_t x[4];
    float value[4];
};
struct UpdateTest_Curve_InternalAxis Curve_InternalAxis = {
    {0U, 100U, 200U, 300U}, {12345.6F, 42.42F, 1000.0F, 65535.9F}};

/**********************************************
 * curve with an external axis                */
struct UpdateTest_Axis_0 {
    uint32_t value[5];
};
struct UpdateTest_Curve_ExternalAxis {
    float value[5];
};
struct UpdateTest_Axis_0 Axis_0 = {{100UL, 200UL, 300UL, 400UL, 500UL}};
struct UpdateTest_Curve_ExternalAxis Curve_ExternalAxis = {
    -99.99F, 12345.6F, 42.42F, 1000.0F, 65535.9F};

/**********************************************
 * map with two internal axes                 */
struct UpdateTest_Map_InternalAxis {
    uint16_t x[4];
    uint16_t y[3];
    uint32_t value[3][4];
};
struct UpdateTest_Map_InternalAxis Map_InternalAxis = {
    {0U, 100U, 200U, 300U},
    {0U, 10U, 20U},
    {{0UL, 1UL, 4UL, 7UL}, {0UL, 2UL, 5UL, 8UL}, {0UL, 3UL, 6UL, 9UL}}};

/**********************************************
 * map with two external axes                 */
struct UpdateTest_Axis_1 {
    uint32_t value[3];
};
struct UpdateTest_Axis_2 {
    uint32_t value[2];
};
struct UpdateTest_Map_ExternalAxis {
    float value[2][3];
};
struct UpdateTest_Axis_1 Axis_1 = {{100UL, 200UL, 300UL}};
struct UpdateTest_Axis_2 Axis_2 = {{0UL, 1UL}};
struct UpdateTest_Map_ExternalAxis Map_ExternalAxis = {
    {{-1.0F, 0.001F, 22.2F}, {-3.0F, -1.5F, 11.0F}}};

/**********************************************
 * ValBlk                                     */
float Characteristic_ValBlk[5] = {1.2, 3.4, 5.6, 7.8, 9.0};

/**********************************************
 * Value                                      */
uint32_t Characteristic_Value = 3;

/**********************************************
 * Complex BLOB data                          */
struct UpdateTest_ComplexBlobData {
    uint32_t value_1[16];
    struct {
        uint16_t value_2_1;
        uint32_t value_2_2;
    } value_2[8];
};
struct UpdateTest_ComplexBlobData Blob_1;

/**********************************************
 * Simple BLOB data                           */
uint8_t Blob_2[256];

/**********************************************
 * Measurement matrix                         */
uint8_t Measurement_Matrix[5][4];

/**********************************************
 * Measurement value                          */
uint16_t Measurement_Value;

/**********************************************
 * Measurement bitfield                       */
struct {
    uint32_t bits_1: 5;
    uint32_t bits_2: 15;
    uint32_t bits_3: 8;
} Measurement_Bitfield;

/**********************************************/

int main(void)
{
    return 0;
}

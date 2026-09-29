typedef enum { RED, GREEN = 5, BLUE } Color;
struct Inner { unsigned char a : 3; unsigned char b : 5; short s; };
struct Outer { int x; float f[4][2]; Inner in; Inner *p; Color c; union { int i; double d; } u; };
namespace ns { struct Cls { static int stat; long long v; virtual int m(); }; int Cls::stat = 3; int Cls::m() { return 1; } Cls clsvar; }
Outer g_outer;
volatile const unsigned short g_cal[8] = {1};
static double s_val = 1.5;
int func(int a) { static int counter; counter += a; return counter + (int)s_val; }
extern "C" int mainCRTStartup() { return func(ns::clsvar.m()) + g_outer.x; }
extern "C" int _fltused = 0;

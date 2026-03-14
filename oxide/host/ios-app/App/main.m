@import Foundation;
int rust_entry(void);
int main(int argc, char **argv)
{
   @autoreleasepool { return rust_entry(); }
}


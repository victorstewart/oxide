#import <Foundation/Foundation.h>

extern int rust_entry(int argc, char **argv);

int main(int argc, char **argv)
{
   @autoreleasepool
   {
      return rust_entry(argc, argv);
   }
}

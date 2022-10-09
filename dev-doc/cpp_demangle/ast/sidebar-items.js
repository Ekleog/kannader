initSidebarItems({"enum":[["ArrayType","The `<array-type>` production."],["BaseUnresolvedName","The `<base-unresolved-name>` production."],["BuiltinType","The `<builtin-type>` production."],["CallOffset","The `<call-offset>` production."],["ClassEnumType","The `<class-enum-type>` production."],["CtorDtorName","The `<ctor-dtor-name>` production."],["Decltype","The `<decltype>` production."],["DestructorName","The `<destructor-name>` production."],["Encoding","The `<encoding>` production."],["ExprPrimary","The `<expr-primary>` production."],["Expression","The `<expression>` production."],["GlobalCtorDtor","A global constructor or destructor."],["LocalName","The `<local-name>` production."],["MangledName","The root AST node, and starting production."],["Name","The `<name>` production."],["NestedName","The `<nested-name>` production."],["OperatorName","The `<operator-name>` production."],["Prefix","The `<prefix>` production."],["PrefixHandle","A reference to a parsed `<prefix>` production."],["RefQualifier","A  production."],["SimpleOperatorName","The `<simple-operator-name>` production."],["SpecialName","The `<special-name>` production."],["StandardBuiltinType","A one of the standard variants of the  production."],["Substitution","The `<substitution>` form: a back-reference to some component we’ve already parsed."],["TemplateArg","A  production."],["TemplateTemplateParamHandle","A reference to a parsed `TemplateTemplateParam`."],["Type","The `<type>` production."],["TypeHandle","A reference to a parsed `Type` production."],["UnqualifiedName","The `<unqualified-name>` production."],["UnresolvedName","The `<unresolved-name>` production."],["UnresolvedType","The `<unresolved-type>` production."],["UnresolvedTypeHandle","A reference to a parsed `<unresolved-type>` production."],["UnscopedName","The `<unscoped-name>` production."],["UnscopedTemplateNameHandle","A handle to an `UnscopedTemplateName`."],["VectorType","The `<vector-type>` production."],["WellKnownComponent","The `<substitution>` variants that are encoded directly in the grammar, rather than as back references to other components in the substitution table."]],"struct":[["ArgScopeStack","An `ArgScopeStack` represents the current function and template demangling scope we are within. As we enter new demangling scopes, we construct new `ArgScopeStack`s whose `prev` references point back to the old ones. These `ArgScopeStack`s are kept on the native stack, and as functions return, they go out of scope and we use the previous `ArgScopeStack`s again."],["BareFunctionType","The `<bare-function-type>` production."],["CloneSuffix"," ::= [ .  ] [ .  ]*"],["CloneTypeIdentifier","The `<clone-type-identifier>` pseudo-terminal."],["ClosureTypeName","The `<closure-type-name>` production."],["CvQualifiers","The `<CV-qualifiers>` production."],["DataMemberPrefix","The `<data-member-prefix>` production."],["Discriminator","The `<discriminator>` production."],["FunctionParam","The  production."],["FunctionType","The `<function-type>` production."],["Identifier","The `<identifier>` pseudo-terminal."],["Initializer","The `<initializer>` production."],["LambdaSig","The `<lambda-sig>` production."],["MemberName","In libiberty, Member and DerefMember expressions have special handling. They parse an `UnqualifiedName` (not an `UnscopedName` as the cxxabi docs say) and optionally a `TemplateArgs` if it is present. We can’t just parse a `Name` or an `UnscopedTemplateName` here because that allows other inputs that libiberty does not."],["NonSubstitution","A handle to a component that is usually substitutable, and lives in the substitutions table, but in this particular case does not qualify for substitutions."],["NvOffset","A non-virtual offset, as described by the  production."],["ParseContext","Common context needed when parsing."],["PointerToMemberType","The `<pointer-to-member-type>` production."],["QualifiedBuiltin","A built-in type with CV-qualifiers."],["ResourceName","The `<resource name>` pseudo-terminal."],["SeqId","A  production encoding a base-36 positive number."],["SimpleId","The `<simple-id>` production."],["SourceName","The `<source-name>` non-terminal."],["TaggedName","The `<tagged-name>` non-terminal."],["TemplateArgs","The `<template-args>` production."],["TemplateParam","The `<template-param>` production."],["TemplateTemplateParam","The `<template-template-param>` production."],["UnnamedTypeName","The `<unnamed-type-name>` production."],["UnresolvedQualifierLevel","The `<unresolved-qualifier-level>` production."],["UnscopedTemplateName","The `<unscoped-template-name>` production."],["VOffset","A virtual offset, as described by the  production."]]});
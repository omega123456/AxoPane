const RULE_MESSAGE_ID = 'nameTheCondition'
const RULE_MESSAGE =
  'Extract this compound condition into a named predicate function or a named variable'
const PREDICATE_FUNCTION_NAME = /^(is|has|can|should)[A-Z0-9_]/
const NEGATION_OPERATOR = '!'
const CONDITION_LOGICAL_OPERATORS = new Set(['&&', '||'])
const COMPARISON_OPERATORS = new Set(['===', '!==', '==', '!=', '<', '>', '<=', '>='])
const MEMBER_CHAIN_MINIMUM_DEPTH = 2
const FUNCTION_NODE_TYPES = new Set([
  'FunctionDeclaration',
  'FunctionExpression',
  'ArrowFunctionExpression',
])
const KEYED_FUNCTION_PARENT_TYPES = new Set(['MethodDefinition', 'Property', 'PropertyDefinition'])
const NAMED_KEY_TYPES = new Set(['Identifier', 'PrivateIdentifier'])
const TRANSPARENT_WRAPPER_TYPES = new Set(['ChainExpression', 'TSNonNullExpression'])

function unwrapChain(node) {
  if (TRANSPARENT_WRAPPER_TYPES.has(node.type)) {
    return unwrapChain(node.expression)
  }
  return node
}

function memberChainDepth(node) {
  const unwrapped = unwrapChain(node)
  if (unwrapped.type !== 'MemberExpression') {
    return 0
  }
  if (unwrapped.object.type === 'ThisExpression') {
    return 0
  }
  return 1 + memberChainDepth(unwrapped.object)
}

function isCall(node) {
  return unwrapChain(node).type === 'CallExpression'
}

function isMemberChain(node) {
  return memberChainDepth(node) >= MEMBER_CHAIN_MINIMUM_DEPTH
}

function isCallOrMemberChain(node) {
  return isCall(node) || isMemberChain(node)
}

function isChainedCall(node) {
  const unwrapped = unwrapChain(node)
  if (unwrapped.type !== 'CallExpression') {
    return false
  }
  return isCallOrMemberChain(unwrapped.callee)
}

function isPlainReference(node) {
  const unwrapped = unwrapChain(node)
  if (unwrapped.type === 'Identifier') {
    return true
  }
  if (unwrapped.type !== 'MemberExpression') {
    return false
  }
  return !unwrapped.computed && memberChainDepth(unwrapped) < MEMBER_CHAIN_MINIMUM_DEPTH
}

function joinsTwoPlainReferences(node) {
  return isPlainReference(node.left) && isPlainReference(node.right)
}

function isCompoundLogical(node) {
  if (node.type !== 'LogicalExpression') {
    return false
  }
  if (!CONDITION_LOGICAL_OPERATORS.has(node.operator)) {
    return false
  }
  return !joinsTwoPlainReferences(node)
}

function isNegatedChainedCall(node) {
  if (node.type !== 'UnaryExpression') {
    return false
  }
  if (node.operator !== NEGATION_OPERATOR) {
    return false
  }
  return isChainedCall(node.argument)
}

function isComparisonAgainstCallOrChain(node) {
  if (node.type !== 'BinaryExpression') {
    return false
  }
  if (!COMPARISON_OPERATORS.has(node.operator)) {
    return false
  }
  return isCallOrMemberChain(node.left) || isCallOrMemberChain(node.right)
}

function isCompoundCondition(node) {
  if (isCompoundLogical(node)) {
    return true
  }
  if (isNegatedChainedCall(node)) {
    return true
  }
  return isComparisonAgainstCallOrChain(node)
}

function keyedFunctionName(parent) {
  if (!NAMED_KEY_TYPES.has(parent.key.type)) {
    return null
  }
  return parent.key.name
}

function declaredVariableName(parent) {
  if (parent.id.type !== 'Identifier') {
    return null
  }
  return parent.id.name
}

function functionName(functionNode) {
  if (functionNode.id) {
    return functionNode.id.name
  }
  const parent = functionNode.parent
  if (parent.type === 'VariableDeclarator') {
    return declaredVariableName(parent)
  }
  if (KEYED_FUNCTION_PARENT_TYPES.has(parent.type)) {
    return keyedFunctionName(parent)
  }
  return null
}

function enclosingFunction(node) {
  let ancestor = node.parent
  while (ancestor && !FUNCTION_NODE_TYPES.has(ancestor.type)) {
    ancestor = ancestor.parent
  }
  return ancestor
}

function typeCheckerOf(context) {
  const services = context.sourceCode.parserServices
  if (!services?.program) {
    return null
  }
  return { checker: services.program.getTypeChecker(), nodeMap: services.esTreeNodeToTSNodeMap }
}

function isBooleanType(checker, type) {
  return checker.isTypeAssignableTo(type, checker.getBooleanType())
}

function isBooleanExpression(context, node) {
  const typeChecker = typeCheckerOf(context)
  if (!typeChecker) {
    return true
  }
  const expressionType = typeChecker.checker.getTypeAtLocation(typeChecker.nodeMap.get(node))
  return isBooleanType(typeChecker.checker, expressionType)
}

function returnsBoolean(context, functionNode) {
  const typeChecker = typeCheckerOf(context)
  if (!typeChecker) {
    return false
  }
  const signature = typeChecker.checker.getSignatureFromDeclaration(
    typeChecker.nodeMap.get(functionNode),
  )
  if (!signature) {
    return false
  }
  return isBooleanType(typeChecker.checker, signature.getReturnType())
}

function isNamedPredicate(context, functionNode) {
  const name = functionName(functionNode)
  if (name === null) {
    return false
  }
  if (PREDICATE_FUNCTION_NAME.test(name)) {
    return true
  }
  return returnsBoolean(context, functionNode)
}

function isInsideNamedPredicate(context, node) {
  const owner = enclosingFunction(node)
  if (!owner) {
    return false
  }
  return isNamedPredicate(context, owner)
}

function reportCondition(context, node) {
  context.report({ node, messageId: RULE_MESSAGE_ID })
}

function checkConditionTest(context, test) {
  if (!isCompoundCondition(test)) {
    return
  }
  reportCondition(context, test)
}

function checkReturnArgument(context, returnStatement) {
  const argument = returnStatement.argument
  if (!argument) {
    return
  }
  if (!isCompoundCondition(argument)) {
    return
  }
  if (isInsideNamedPredicate(context, returnStatement)) {
    return
  }
  if (!isBooleanExpression(context, argument)) {
    return
  }
  reportCondition(context, argument)
}

export const namedConditionsRule = {
  meta: {
    type: 'suggestion',
    docs: {
      description:
        'Require compound conditions in if, while, ternary and return positions to read as one name',
    },
    schema: [],
    messages: { [RULE_MESSAGE_ID]: RULE_MESSAGE },
  },
  create(context) {
    return {
      IfStatement: (statement) => checkConditionTest(context, statement.test),
      WhileStatement: (statement) => checkConditionTest(context, statement.test),
      ConditionalExpression: (expression) => checkConditionTest(context, expression.test),
      ReturnStatement: (statement) => checkReturnArgument(context, statement),
    }
  },
}

export const localRulesPlugin = {
  rules: { 'named-conditions': namedConditionsRule },
}

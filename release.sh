S=$(git status -s)

if ! [ -z "$S" ]; then
    echo "please commit and push"
    exit 1
fi

git checkout gh-pages

if [ $? -ne 0 ]; then
    exit $?
fi

if [ $? -ne 0 ]; then
    exit $?
fi

mkdir -p dist

if [ $? -ne 0 ]; then
    exit $?
fi

cd dist

if [ $? -ne 0 ]; then
    exit $?
fi

helm package ../chart

if [ $? -ne 0 ]; then
    exit $?
fi

helm repo index .

if [ $? -ne 0 ]; then
    exit $?
fi

cd ..

if [ $? -ne 0 ]; then
    exit $?
fi

rm -rf `ls -a | grep -v . | grep -v .. | grep -v .git | grep -v .gitignore | grep -v dist`

if [ $? -ne 0 ]; then
    exit $?
fi

# mv ./dist/* .

# if [ $? -ne 0 ]; then
#     exit $?
# fi

git add .

if [ $? -ne 0 ]; then
    exit $?
fi

git push

if [ $? -ne 0 ]; then
    exit $?
fi

git checkout master

if [ $? -ne 0 ]; then
    exit $?
fi

git reset HEAD

if [ $? -ne 0 ]; then
    exit $?
fi

git checkout .

rm -rf dist

S=$(git status -s)

if ! [ -z "$S" ]; then
    echo "please commit and push"
    exit 1
fi

echo "change branch to gh-pages"

git checkout gh-pages

if [ $? -ne 0 ]; then
    exit $?
fi

echo "copy chart directory from master branch"

git checkout master chart

if [ $? -ne 0 ]; then
    exit $?
fi

echo "package chart"

helm package chart

if [ $? -ne 0 ]; then
    exit $?
fi

echo "update repo index"

mv index.yaml prev_index.yaml

if [ $? -ne 0 ]; then
    exit $?
fi

helm repo index --merge prev_index.yaml .

if [ $? -ne 0 ]; then
    exit $?
fi

# rm -rf prev_index.yaml

# if [ $? -ne 0 ]; then
#     exit $?
# fi

echo "remove chart directory"

# rm -rf `ls -a | grep -v .git | grep -v .gitignore | grep -v dist`
rm -rf chart

if [ $? -ne 0 ]; then
    exit $?
fi

# mv ./dist/* .

# if [ $? -ne 0 ]; then
#     exit $?
# fi

echo "add files to stage"

git add .

if [ $? -ne 0 ]; then
    exit $?
fi

echo "commit"

git commit -m "release"

if [ $? -ne 0 ]; then
    exit $?
fi

echo "push"

git push

if [ $? -ne 0 ]; then
    git reset HEAD
    git checkout .
    exit $?
fi

echo "change branch to master"

git checkout master

if [ $? -ne 0 ]; then
    exit $?
fi
